mod event_loop;
mod monitors;
mod serialization;

pub use event_loop::run_event_driven;

use crate::state_saver::StateSaverHandle;
use anyhow::Result;
use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::core::{GPUSlot, GPU, UUID};
use gflow::tmux::disable_pipe_pane_for_job;
use nvml_wrapper::Nvml;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;

pub type SharedState = Arc<RwLock<SchedulerRuntime>>;

/// Wrapper to make Arc<dyn Executor> compatible with Box<dyn Executor>
struct ArcExecutorWrapper(Arc<dyn Executor>);

impl Executor for ArcExecutorWrapper {
    fn execute(&self, job: &Job) -> Result<()> {
        self.0.execute(job)
    }
}

/// Runtime adapter for Scheduler with system integration
pub struct SchedulerRuntime {
    scheduler: Scheduler,
    nvml: Option<Nvml>,
    executor: Arc<dyn Executor>, // Shared executor for lock-free job execution
    dirty: bool,                 // Tracks if state has changed since last save
    state_saver: Option<StateSaverHandle>, // Handle for async background state persistence
    state_writable: bool,        // False when state load/migration failed
    state_load_error: Option<String>,
    state_backup_path: Option<PathBuf>,
    journal_path: PathBuf,
    journal_writable: bool,
    journal_error: Option<String>,
    journal_applied: bool,
}

impl SchedulerRuntime {
    /// Create a new scheduler runtime with state loading and NVML initialization
    pub fn with_state_path(
        executor: Box<dyn Executor>,
        state_dir: PathBuf,
        allowed_gpu_indices: Option<Vec<u32>>,
    ) -> anyhow::Result<Self> {
        // Try to initialize NVML, but continue without it if it fails
        let (nvml, gpu_slots) = match Nvml::init() {
            Ok(nvml) => {
                let gpu_slots = Self::get_gpus(&nvml);
                (Some(nvml), gpu_slots)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize NVML: {}. Running without GPU support.",
                    e
                );
                (None, HashMap::new())
            }
        };

        // Validate and filter allowed GPU indices
        let validated_gpu_indices = if let Some(ref allowed) = allowed_gpu_indices {
            let detected_count = gpu_slots.len();
            let (valid, invalid): (Vec<_>, Vec<_>) = allowed
                .iter()
                .copied()
                .partition(|&idx| idx < detected_count as u32);

            if !invalid.is_empty() {
                tracing::warn!(
                    "Invalid GPU indices {:?} specified (only {} GPUs detected). These will be filtered out.",
                    invalid,
                    detected_count
                );
            }

            if valid.is_empty() {
                tracing::warn!(
                    "No valid GPU indices remaining after filtering. Allowing all GPUs."
                );
                None
            } else {
                tracing::info!("GPU restriction enabled: allowing only GPUs {:?}", valid);
                Some(valid)
            }
        } else {
            None
        };

        let total_memory_mb = Self::get_total_system_memory_mb();

        // Store executor in Arc for lock-free access during job execution
        let executor_arc: Arc<dyn Executor> = Arc::from(executor);

        // Clone Arc for scheduler
        let executor_for_scheduler: Box<dyn Executor> =
            Box::new(ArcExecutorWrapper(executor_arc.clone()));

        let state_file = state_dir.join("state.json");
        let journal_path = state_dir.join("state.journal.jsonl");
        let scheduler = SchedulerBuilder::new()
            .with_executor(executor_for_scheduler)
            .with_gpu_slots(gpu_slots)
            .with_state_path(state_file)
            .with_total_memory_mb(total_memory_mb)
            .with_allowed_gpu_indices(validated_gpu_indices)
            .build();

        let mut runtime = Self {
            scheduler,
            nvml,
            executor: executor_arc,
            dirty: false,
            state_saver: None,
            state_writable: true,
            state_load_error: None,
            state_backup_path: None,
            journal_path,
            journal_writable: false,
            journal_error: None,
            journal_applied: false,
        };
        runtime.load_state();
        runtime.init_journal();
        Ok(runtime)
    }

    pub fn state_writable(&self) -> bool {
        self.state_writable
    }

    pub fn journal_writable(&self) -> bool {
        self.journal_writable
    }

    pub fn persistence_mode(&self) -> &'static str {
        if self.state_writable {
            "state"
        } else if self.journal_writable {
            "journal"
        } else {
            "read_only"
        }
    }

    pub fn can_mutate(&self) -> bool {
        self.state_writable || self.journal_writable
    }

    pub fn state_load_error(&self) -> Option<&str> {
        self.state_load_error.as_deref()
    }

    pub fn state_backup_path(&self) -> Option<&std::path::Path> {
        self.state_backup_path.as_deref()
    }

    pub fn journal_path(&self) -> &std::path::Path {
        &self.journal_path
    }

    pub fn journal_error(&self) -> Option<&str> {
        self.journal_error.as_deref()
    }

    /// Save scheduler state to disk asynchronously
    pub async fn save_state(&mut self) {
        if !self.state_writable {
            self.append_journal_snapshot().await;
            return;
        }

        let state_dir = self
            .scheduler
            .state_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        // Use MessagePack format for better performance and smaller file size
        match serialization::save_state(
            &self.scheduler,
            state_dir,
            serialization::SerializationFormat::MessagePack,
        ) {
            Ok(_) => {
                // If journal was applied, truncate it after successful state save
                if self.journal_applied {
                    if let Err(e) = tokio::fs::OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .open(&self.journal_path)
                        .await
                    {
                        tracing::warn!(
                            "Failed to truncate journal file {}: {}",
                            self.journal_path.display(),
                            e
                        );
                    } else {
                        self.journal_applied = false;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to save scheduler state: {}", e);
            }
        }
    }

    /// Mark state as dirty without saving immediately
    fn mark_dirty(&mut self) {
        if !(self.state_writable || self.journal_writable) {
            return;
        }
        self.dirty = true;
        // Notify state saver asynchronously (if configured)
        if let Some(ref saver) = self.state_saver {
            saver.mark_dirty();
        }
    }

    /// Save state only if dirty flag is set, then clear flag
    pub async fn save_state_if_dirty(&mut self) {
        if self.dirty {
            self.save_state().await;
            // If persistence failed, keep dirty to retry later.
            if self.state_writable || self.journal_writable {
                self.dirty = false;
            }
        }
    }

    /// Set the state saver handle for async background persistence
    ///
    /// This should be called after creating the SchedulerRuntime to enable
    /// background state saves. The handle allows the scheduler to notify
    /// the state saver task when state changes occur.
    pub fn set_state_saver(&mut self, saver: StateSaverHandle) {
        let should_kick = self.dirty;
        self.state_saver = Some(saver);
        if should_kick {
            if let Some(ref saver) = self.state_saver {
                saver.mark_dirty();
            }
        }
    }

    /// Load scheduler state from disk
    pub fn load_state(&mut self) {
        self.state_writable = true;
        self.state_load_error = None;
        self.state_backup_path = None;
        self.journal_applied = false;

        let state_dir = self
            .scheduler
            .state_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let mut loaded: Option<Scheduler> = None;

        // Try to load state using the new serialization module (supports both formats)
        match serialization::load_state_auto(&state_dir) {
            Ok(Some(loaded_scheduler)) => {
                // Apply migrations if needed
                match gflow::core::migrations::migrate_state(loaded_scheduler) {
                    Ok(migrated) => {
                        loaded = Some(migrated);
                    }
                    Err(e) => {
                        // Backup the state file on migration failure
                        let json_path = state_dir.join("state.json");
                        let msgpack_path = state_dir.join("state.msgpack");
                        let backup_path = if msgpack_path.exists() {
                            &msgpack_path
                        } else {
                            &json_path
                        };

                        let (backup_result, backup_err) = backup_state_file(backup_path, "backup");
                        if let Some(err) = backup_err {
                            tracing::error!("Failed to backup state file: {}", err);
                        }

                        self.state_writable = false;
                        self.state_load_error = Some(format!(
                            "State migration failed: {e}. gflowd entered recovery mode (journal) to avoid overwriting your state file."
                        ));
                        self.state_backup_path = backup_result;
                        tracing::error!("{}", self.state_load_error.as_deref().unwrap());

                        // Try to load without migration for read-only mode
                        if let Ok(Some(scheduler)) = serialization::load_state_auto(&state_dir) {
                            loaded = Some(scheduler);
                        }
                    }
                }
            }
            Ok(None) => {
                tracing::info!(
                    "No existing state file found in {}, starting fresh",
                    state_dir.display()
                );
            }
            Err(e) => {
                // Failed to load state - enter recovery mode
                let json_path = state_dir.join("state.json");
                let msgpack_path = state_dir.join("state.msgpack");
                let failed_path = if msgpack_path.exists() {
                    &msgpack_path
                } else {
                    &json_path
                };

                let (backup_result, backup_err) = backup_state_file(failed_path, "corrupt");
                if let Some(err) = backup_err {
                    tracing::error!("Failed to backup corrupted state file: {}", err);
                }

                self.state_writable = false;
                self.state_load_error = Some(format!(
                    "Failed to load state file from {}: {e}. gflowd entered recovery mode (journal) to avoid overwriting your state file.",
                    state_dir.display()
                ));
                self.state_backup_path = backup_result;
                tracing::error!("{}", self.state_load_error.as_deref().unwrap());

                // Start from a non-overlapping job ID range to reduce future merge conflicts.
                self.scheduler.set_next_job_id(2_000_000_000);
            }
        }

        // Apply journal if needed
        let legacy_json_path = state_dir.join("state.json");
        if should_apply_journal(&legacy_json_path, &self.journal_path) {
            if let Some((snapshot, ts)) = load_last_journal_snapshot(&self.journal_path) {
                tracing::warn!(
                    "Loading scheduler state from journal snapshot (ts={}) at {}",
                    ts,
                    self.journal_path.display()
                );
                loaded = Some(snapshot);
                self.journal_applied = true;
                if self.state_writable {
                    // Ensure we rewrite state so that it incorporates the journaled state.
                    self.dirty = true;
                }
            }
        }

        if let Some(scheduler) = loaded {
            self.apply_loaded_scheduler(scheduler);
        }

        self.reinitialize_runtime_resources();
    }

    fn init_journal(&mut self) {
        self.journal_writable = false;
        self.journal_error = None;

        if let Some(parent) = self.journal_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.journal_error = Some(format!("Failed to create journal dir: {e}"));
                tracing::error!("{}", self.journal_error.as_deref().unwrap());
                return;
            }
        }

        match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&self.journal_path)
        {
            Ok(_) => {
                self.journal_writable = true;
            }
            Err(e) => {
                self.journal_error = Some(format!(
                    "Failed to open journal file {}: {e}",
                    self.journal_path.display()
                ));
                tracing::error!("{}", self.journal_error.as_deref().unwrap());
            }
        }
    }

    fn apply_loaded_scheduler(&mut self, loaded: Scheduler) {
        let next_id = loaded.next_job_id();
        let allowed_gpus = loaded.allowed_gpu_indices().cloned();

        self.scheduler.version = loaded.version;
        self.scheduler.jobs = loaded.jobs;
        self.scheduler.set_next_job_id(next_id);
        self.scheduler.set_allowed_gpu_indices(allowed_gpus);
        self.scheduler.rebuild_user_jobs_index();
    }

    fn reinitialize_runtime_resources(&mut self) {
        // Re-initialize NVML and GPU slots (fresh detection)
        match Nvml::init() {
            Ok(nvml) => {
                self.scheduler.update_gpu_slots(Self::get_gpus(&nvml));
                self.nvml = Some(nvml);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize NVML during state load: {}. Running without GPU support.",
                    e
                );
                self.scheduler.update_gpu_slots(HashMap::new());
                self.nvml = None;
            }
        }

        // Re-initialize memory tracking with current system values
        let total_memory_mb = Self::get_total_system_memory_mb();
        self.scheduler.update_memory(total_memory_mb);
        self.scheduler.refresh_available_memory();
    }

    async fn append_journal_snapshot(&mut self) {
        if !self.journal_writable {
            tracing::error!(
                "Refusing to persist state: state.json is not writable and journal is not writable"
            );
            return;
        }

        if let Some(parent) = self.journal_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                tracing::error!(
                    "Failed to create journal directory {}: {}",
                    parent.display(),
                    e
                );
                self.journal_writable = false;
                self.journal_error = Some(format!("Failed to create journal dir: {e}"));
                return;
            }
        }

        #[derive(serde::Serialize)]
        struct JournalEntry<'a> {
            ts: u64,
            kind: &'static str,
            scheduler: &'a Scheduler,
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = JournalEntry {
            ts,
            kind: "snapshot",
            scheduler: &self.scheduler,
        };

        let line = match serde_json::to_string(&entry) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to serialize journal entry: {}", e);
                return;
            }
        };

        // For simplicity and bounded size, keep only a single snapshot in the journal.
        // Write to a temp file and atomically rename into place.
        let tmp_path = self.journal_path.with_extension("jsonl.tmp");
        match tokio::fs::File::create(&tmp_path).await {
            Ok(mut file) => {
                if let Err(e) =
                    tokio::io::AsyncWriteExt::write_all(&mut file, format!("{line}\n").as_bytes())
                        .await
                {
                    tracing::error!(
                        "Failed to write journal snapshot to {}: {}",
                        tmp_path.display(),
                        e
                    );
                    return;
                }

                if let Err(e) = file.sync_all().await {
                    tracing::warn!(
                        "Failed to fsync journal temp file {}: {}",
                        tmp_path.display(),
                        e
                    );
                }

                if let Err(e) = tokio::fs::rename(&tmp_path, &self.journal_path).await {
                    // Best-effort fallback: remove destination then retry.
                    let _ = tokio::fs::remove_file(&self.journal_path).await;
                    if let Err(e2) = tokio::fs::rename(&tmp_path, &self.journal_path).await {
                        tracing::error!(
                            "Failed to move journal snapshot from {} to {}: {} (retry error: {})",
                            tmp_path.display(),
                            self.journal_path.display(),
                            e,
                            e2
                        );
                        self.journal_writable = false;
                        self.journal_error =
                            Some(format!("Failed to finalize journal snapshot: {e2}"));
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to create journal temp file {}: {}",
                    tmp_path.display(),
                    e
                );
                self.journal_writable = false;
                self.journal_error = Some(format!("Failed to create journal temp file: {e}"));
            }
        };
    }

    /// Refresh GPU slot availability using NVML
    fn refresh_gpu_slots(&mut self) {
        let running_gpu_indices: std::collections::HashSet<u32> = self
            .scheduler
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Running)
            .filter_map(|j| j.gpu_ids.as_ref())
            .flat_map(|ids| ids.iter().copied())
            .collect();

        if let Some(nvml) = &self.nvml {
            if let Ok(device_count) = nvml.device_count() {
                for i in 0..device_count {
                    if let Ok(device) = nvml.device_by_index(i) {
                        if let Ok(uuid) = device.uuid() {
                            if let Some(slot) = self.scheduler.gpu_slots_mut().get_mut(&uuid) {
                                let is_free_in_scheduler =
                                    !running_gpu_indices.contains(&slot.index);
                                let is_free_in_nvml = device
                                    .running_compute_processes()
                                    .is_ok_and(|procs| procs.is_empty());
                                slot.available = is_free_in_scheduler && is_free_in_nvml;

                                // Set reason if GPU is occupied by non-gflow process
                                if is_free_in_scheduler && !is_free_in_nvml {
                                    slot.reason = Some("Unmanaged".to_string());
                                } else {
                                    slot.reason = None;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get total system memory in MB by reading /proc/meminfo (Linux)
    fn get_total_system_memory_mb() -> u64 {
        // Try to read /proc/meminfo on Linux
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    // MemTotal:       32864256 kB
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb / 1024; // Convert KB to MB
                        }
                    }
                }
            }
        }

        // Fallback: assume 16GB if we can't read system memory
        tracing::warn!("Could not read system memory from /proc/meminfo, assuming 16GB");
        16 * 1024
    }

    // Job mutation methods

    pub async fn submit_job(&mut self, job: Job) -> (u32, String, Job) {
        let (job_id, run_name) = self.scheduler.submit_job(job);
        self.mark_dirty();

        // Clone job for return
        let job_clone = self
            .scheduler
            .get_job(job_id)
            .cloned()
            .expect("Job should exist after submission");

        (job_id, run_name, job_clone)
    }

    /// Submit multiple jobs in a batch
    pub async fn submit_jobs(
        &mut self,
        jobs: Vec<Job>,
    ) -> (Vec<(u32, String, String)>, Vec<Job>, u32) {
        let mut results = Vec::with_capacity(jobs.len());
        let mut submitted_jobs = Vec::with_capacity(jobs.len());

        for job in jobs {
            let submitted_by = job.submitted_by.to_string();
            let (job_id, run_name) = self.scheduler.submit_job(job);
            results.push((job_id, run_name, submitted_by));

            if let Some(job) = self.scheduler.get_job(job_id) {
                submitted_jobs.push(job.clone());
            }
        }

        self.mark_dirty();
        let next_id = self.scheduler.next_job_id();
        (results, submitted_jobs, next_id)
    }

    pub async fn finish_job(&mut self, job_id: u32) -> bool {
        if let Some((should_close_tmux, run_name)) = self.scheduler.finish_job(job_id) {
            self.mark_dirty();

            if let Some(name) = run_name {
                if should_close_tmux {
                    // Close tmux session if auto_close is enabled (this also disables pipe-pane)
                    tracing::info!("Auto-closing tmux session '{}' for job {}", name, job_id);
                    if let Err(e) = gflow::tmux::kill_session(&name) {
                        tracing::warn!("Failed to auto-close tmux session '{}': {}", name, e);
                    }
                } else {
                    // Disable pipe-pane to prevent process leaks (keep session alive for user inspection)
                    disable_pipe_pane_for_job(job_id, &name, false);
                }
            }

            true
        } else {
            false
        }
    }

    pub async fn fail_job(&mut self, job_id: u32) -> bool {
        // Get run_name before modifying state (needed for PipePane cleanup)
        let run_name = self
            .scheduler
            .get_job(job_id)
            .and_then(|j| j.run_name.clone());

        let result = self.scheduler.fail_job(job_id);
        if result {
            // Note: Cascade cancellation is now handled by the cascade_handler event handler
            self.mark_dirty();

            // Disable PipePane to prevent process leaks (keep session alive for user inspection)
            if let Some(name) = run_name {
                disable_pipe_pane_for_job(job_id, &name, false);
            }
        }
        result
    }

    pub async fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some((was_running, run_name)) = self.scheduler.cancel_job(job_id, None) {
            // Note: Cascade cancellation is now handled by the cascade_handler event handler
            self.mark_dirty();

            // If the job was running, send Ctrl-C to gracefully interrupt it, then disable PipePane
            if was_running {
                if let Some(name) = run_name {
                    if let Err(e) = gflow::tmux::send_ctrl_c(&name) {
                        tracing::error!("Failed to send C-c to tmux session {}: {}", name, e);
                    }

                    // Wait a moment for graceful shutdown, then disable PipePane
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    disable_pipe_pane_for_job(job_id, &name, false);
                }
            }
            true
        } else {
            false
        }
    }

    pub async fn hold_job(&mut self, job_id: u32) -> bool {
        let result = self.scheduler.hold_job(job_id);
        if result {
            self.mark_dirty();
        }
        result
    }

    pub async fn release_job(&mut self, job_id: u32) -> bool {
        let result = self.scheduler.release_job(job_id);
        if result {
            self.mark_dirty();
        }
        result
    }

    /// Update max_concurrent for a specific job
    pub fn update_job_max_concurrent(&mut self, job_id: u32, max_concurrent: usize) -> Option<Job> {
        if let Some(job) = self.scheduler.get_job_mut(job_id) {
            job.max_concurrent = Some(max_concurrent);
            let job_clone = job.clone();
            self.mark_dirty();
            Some(job_clone)
        } else {
            None
        }
    }

    /// Update job parameters
    /// Returns Ok((updated_job, updated_fields)) on success, Err(error_message) on failure
    pub async fn update_job(
        &mut self,
        job_id: u32,
        request: super::server::UpdateJobRequest,
    ) -> Result<(Job, Vec<String>), String> {
        let mut updated_fields = Vec::new();

        // Validate the update first
        let new_deps = request.depends_on_ids.as_deref();
        self.scheduler.validate_job_update(job_id, new_deps)?;

        // Get mutable reference to the job
        let job = self
            .scheduler
            .get_job_mut(job_id)
            .ok_or_else(|| format!("Job {} not found", job_id))?;

        // Apply updates
        if let Some(command) = request.command {
            job.command = Some(command);
            updated_fields.push("command".to_string());
        }

        if let Some(script) = request.script {
            job.script = Some(script);
            updated_fields.push("script".to_string());
        }

        if let Some(gpus) = request.gpus {
            job.gpus = gpus;
            updated_fields.push("gpus".to_string());
        }

        if let Some(conda_env) = request.conda_env {
            job.conda_env = conda_env.map(compact_str::CompactString::from);
            updated_fields.push("conda_env".to_string());
        }

        if let Some(priority) = request.priority {
            job.priority = priority;
            updated_fields.push("priority".to_string());
        }

        if let Some(parameters) = request.parameters {
            job.parameters = parameters;
            updated_fields.push("parameters".to_string());
        }

        if let Some(time_limit) = request.time_limit {
            job.time_limit = time_limit;
            updated_fields.push("time_limit".to_string());
        }

        if let Some(memory_limit_mb) = request.memory_limit_mb {
            job.memory_limit_mb = memory_limit_mb;
            updated_fields.push("memory_limit_mb".to_string());
        }

        if let Some(depends_on_ids) = request.depends_on_ids {
            job.depends_on_ids = depends_on_ids.into();
            updated_fields.push("depends_on_ids".to_string());
        }

        if let Some(dependency_mode) = request.dependency_mode {
            job.dependency_mode = dependency_mode;
            updated_fields.push("dependency_mode".to_string());
        }

        if let Some(auto_cancel) = request.auto_cancel_on_dependency_failure {
            job.auto_cancel_on_dependency_failure = auto_cancel;
            updated_fields.push("auto_cancel_on_dependency_failure".to_string());
        }

        if let Some(max_concurrent) = request.max_concurrent {
            job.max_concurrent = max_concurrent;
            updated_fields.push("max_concurrent".to_string());
        }

        // Clone the job before marking dirty
        let updated_job = job.clone();

        // Mark state as dirty for persistence
        self.mark_dirty();

        // Return cloned job and list of updated fields
        Ok((updated_job, updated_fields))
    }

    // Read-only delegated methods (no state changes)

    pub fn resolve_dependency(&self, username: &str, shorthand: &str) -> Option<u32> {
        self.scheduler.resolve_dependency(username, shorthand)
    }

    pub fn info(&self) -> gflow::core::info::SchedulerInfo {
        self.scheduler.info()
    }

    pub fn gpu_slots_count(&self) -> usize {
        self.scheduler.gpu_slots_count()
    }

    pub fn set_allowed_gpu_indices(&mut self, indices: Option<Vec<u32>>) {
        self.scheduler.set_allowed_gpu_indices(indices);
        self.mark_dirty();
    }

    // Direct access to jobs for server handlers
    pub fn jobs(&self) -> &Vec<Job> {
        &self.scheduler.jobs
    }

    // Get a job by ID
    pub fn get_job(&self, job_id: u32) -> Option<&Job> {
        self.scheduler.get_job(job_id)
    }

    // Debug/metrics accessors
    pub fn next_job_id(&self) -> u32 {
        self.scheduler.next_job_id()
    }

    pub fn validate_no_circular_dependency(
        &self,
        new_job_id: u32,
        dependency_ids: &[u32],
    ) -> Result<(), String> {
        self.scheduler
            .validate_no_circular_dependency(new_job_id, dependency_ids)
    }

    pub fn total_memory_mb(&self) -> u64 {
        self.scheduler.total_memory_mb()
    }

    pub fn available_memory_mb(&self) -> u64 {
        self.scheduler.available_memory_mb()
    }

    // GPU Reservation methods
    pub fn create_reservation(
        &mut self,
        user: compact_str::CompactString,
        gpu_spec: gflow::core::reservation::GpuSpec,
        start_time: std::time::SystemTime,
        duration: std::time::Duration,
    ) -> anyhow::Result<u32> {
        let result = self
            .scheduler
            .create_reservation(user, gpu_spec, start_time, duration)?;
        self.mark_dirty();
        Ok(result)
    }

    pub fn get_reservation(&self, id: u32) -> Option<&gflow::core::reservation::GpuReservation> {
        self.scheduler.get_reservation(id)
    }

    pub fn cancel_reservation(&mut self, id: u32) -> anyhow::Result<()> {
        self.scheduler.cancel_reservation(id)?;
        self.mark_dirty();
        Ok(())
    }

    pub fn list_reservations(
        &self,
        user_filter: Option<&str>,
        status_filter: Option<gflow::core::reservation::ReservationStatus>,
        active_only: bool,
    ) -> Vec<&gflow::core::reservation::GpuReservation> {
        self.scheduler
            .list_reservations(user_filter, status_filter, active_only)
    }
}

fn should_apply_journal(state_path: &std::path::Path, journal_path: &std::path::Path) -> bool {
    let Ok(j_meta) = std::fs::metadata(journal_path) else {
        return false;
    };
    if j_meta.len() == 0 {
        return false;
    }
    let Ok(j_mtime) = j_meta.modified() else {
        return true;
    };

    let Ok(s_meta) = std::fs::metadata(state_path) else {
        return true;
    };
    let Ok(s_mtime) = s_meta.modified() else {
        return true;
    };

    j_mtime >= s_mtime
}

fn load_last_journal_snapshot(journal_path: &std::path::Path) -> Option<(Scheduler, u64)> {
    #[derive(serde::Deserialize)]
    struct Entry {
        ts: u64,
        kind: String,
        scheduler: Scheduler,
    }

    // Single-snapshot format: first line is the snapshot JSON.
    let content = std::fs::read_to_string(journal_path).ok()?;
    let line = content.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let entry = serde_json::from_str::<Entry>(line).ok()?;
    if entry.kind != "snapshot" {
        return None;
    }
    Some((entry.scheduler, entry.ts))
}

fn backup_state_file(
    path: &std::path::Path,
    kind: &str,
) -> (Option<PathBuf>, Option<anyhow::Error>) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // `state.json` -> `state.json.backup.<ts>` (keeps the original file intact)
    let backup_path = path.with_extension(format!("json.{kind}.{ts}"));
    match std::fs::copy(path, &backup_path) {
        Ok(_) => (Some(backup_path), None),
        Err(e) => (None, Some(anyhow::anyhow!(e))),
    }
}

impl GPU for SchedulerRuntime {
    fn get_gpus(nvml: &Nvml) -> HashMap<UUID, GPUSlot> {
        let mut gpu_slots = HashMap::new();
        let device_count = nvml.device_count().unwrap_or(0);
        for i in 0..device_count {
            if let Ok(device) = nvml.device_by_index(i) {
                if let Ok(uuid) = device.uuid() {
                    gpu_slots.insert(
                        uuid,
                        GPUSlot {
                            available: true,
                            index: i,
                            reason: None,
                        },
                    );
                }
            }
        }
        gpu_slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::executor::Executor;
    use gflow::core::job::{Job, JobState};

    struct NoopExecutor;

    impl Executor for NoopExecutor {
        fn execute(&self, _job: &Job) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn enters_journal_mode_and_does_not_overwrite_state_on_migration_failure() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.json");

        // Use a future version to force `migrate_state()` to fail.
        let state_json = serde_json::json!({
            "version": 999,
            "jobs": [
                {
                    "id": 1,
                    "state": "Queued",
                    "script": null,
                    "command": "echo test",
                    "gpus": 0,
                    "conda_env": null,
                    "run_dir": ".",
                    "priority": 0,
                    "depends_on": null,
                    "depends_on_ids": [],
                    "dependency_mode": null,
                    "auto_cancel_on_dependency_failure": true,
                    "task_id": null,
                    "time_limit": null,
                    "memory_limit_mb": null,
                    "submitted_by": "tester",
                    "redone_from": null,
                    "auto_close_tmux": false,
                    "parameters": {},
                    "group_id": null,
                    "max_concurrent": null,
                    "run_name": null,
                    "gpu_ids": null,
                    "submitted_at": null,
                    "started_at": null,
                    "finished_at": null,
                    "reason": null
                }
            ],
            "state_path": "state.json",
            "next_job_id": 2,
            "allowed_gpu_indices": null
        })
        .to_string();
        std::fs::write(&state_path, &state_json).unwrap();
        let original = std::fs::read_to_string(&state_path).unwrap();

        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
        )
        .unwrap();

        assert!(!runtime.state_writable());
        assert!(runtime.state_load_error().is_some());
        assert!(runtime.state_backup_path().is_some_and(|p| p.exists()));
        assert!(runtime.journal_writable());
        assert_eq!(runtime.persistence_mode(), "journal");

        // State is still visible for inspection.
        let job = runtime.get_job(1).unwrap();
        assert_eq!(job.state, JobState::Queued);

        // `save_state()` should append to journal and not overwrite the original file.
        runtime.save_state().await;
        let after = std::fs::read_to_string(&state_path).unwrap();
        assert_eq!(after, original);

        let journal_path = dir.path().join("state.journal.jsonl");
        let journal = std::fs::read_to_string(&journal_path).unwrap();
        assert!(journal.contains("\"kind\":\"snapshot\""));
        assert!(journal.contains("\"jobs\""));

        // Sanity: scheduler is still usable for read paths (no panic on info).
        let _info = runtime.info();
    }

    #[tokio::test]
    async fn prefers_newer_journal_snapshot_and_truncates_after_state_save() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.json");
        let journal_path = dir.path().join("state.journal.jsonl");

        let job = serde_json::json!({
            "id": 1,
            "state": "Queued",
            "script": null,
            "command": "echo test",
            "gpus": 0,
            "conda_env": null,
            "run_dir": ".",
            "priority": 0,
            "depends_on": null,
            "depends_on_ids": [],
            "dependency_mode": null,
            "auto_cancel_on_dependency_failure": true,
            "task_id": null,
            "time_limit": null,
            "memory_limit_mb": null,
            "submitted_by": "tester",
            "redone_from": null,
            "auto_close_tmux": false,
            "parameters": {},
            "group_id": null,
            "max_concurrent": null,
            "run_name": null,
            "gpu_ids": null,
            "submitted_at": null,
            "started_at": null,
            "finished_at": null,
            "reason": null
        });

        let state_json = serde_json::json!({
            "version": gflow::core::migrations::CURRENT_VERSION,
            "jobs": [ job ],
            "state_path": "state.json",
            "next_job_id": 2,
            "allowed_gpu_indices": null
        })
        .to_string();
        std::fs::write(&state_path, &state_json).unwrap();

        // Journal snapshot shows the job as Finished.
        let mut finished_job = serde_json::json!(job);
        finished_job["state"] = serde_json::Value::String("Finished".to_string());
        let journal_entry = serde_json::json!({
            "ts": 9999999999u64,
            "kind": "snapshot",
            "scheduler": {
                "version": gflow::core::migrations::CURRENT_VERSION,
                "jobs": [ finished_job ],
                "state_path": "state.json",
                "next_job_id": 2,
                "allowed_gpu_indices": null
            }
        })
        .to_string();
        std::fs::write(&journal_path, format!("{journal_entry}\n")).unwrap();

        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
        )
        .unwrap();

        assert_eq!(runtime.persistence_mode(), "state");
        assert_eq!(runtime.get_job(1).unwrap().state, JobState::Finished);

        // load_state marked the runtime dirty, so this should consolidate into state.json and truncate the journal.
        runtime.save_state_if_dirty().await;

        let journal_after = std::fs::read_to_string(&journal_path).unwrap();
        assert!(journal_after.trim().is_empty());

        // State is now saved in MessagePack format
        let msgpack_path = dir.path().join("state.msgpack");
        assert!(msgpack_path.exists(), "state.msgpack should exist");

        // Verify the state was saved correctly by loading it back
        let state_bytes = std::fs::read(&msgpack_path).unwrap();
        let loaded_scheduler: Scheduler = rmp_serde::from_slice(&state_bytes).unwrap();
        assert_eq!(
            loaded_scheduler.get_job(1).unwrap().state,
            JobState::Finished
        );
    }
}
