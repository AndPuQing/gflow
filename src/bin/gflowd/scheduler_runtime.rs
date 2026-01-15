use crate::state_saver::StateSaverHandle;
use anyhow::Result;
use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::core::{GPUSlot, GPU, UUID};
use nvml_wrapper::Nvml;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Notify, RwLock};

pub type SharedState = Arc<RwLock<SchedulerRuntime>>;

/// Shared notification handle to wake up the scheduler
pub type SchedulerNotify = Arc<Notify>;

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

        let state_file = state_dir.join("gflow.json");
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
        };
        runtime.load_state();
        Ok(runtime)
    }

    /// Save scheduler state to disk asynchronously
    pub async fn save_state(&self) {
        let path = self.scheduler.state_path();
        let tmp_path = path.with_extension("json.tmp");

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                tracing::error!(
                    "Failed to create state directory {}: {}",
                    parent.display(),
                    e
                );
                return;
            }
        }

        match serde_json::to_string_pretty(&self.scheduler) {
            Ok(json) => {
                match tokio::fs::File::create(&tmp_path).await {
                    Ok(mut file) => {
                        match tokio::io::AsyncWriteExt::write_all(&mut file, json.as_bytes()).await
                        {
                            Ok(_) => {
                                // Atomic rename
                                if let Err(e) = tokio::fs::rename(&tmp_path, path).await {
                                    tracing::error!(
                                        "Failed to rename state file from {} to {}: {}",
                                        tmp_path.display(),
                                        path.display(),
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to write state to {}: {}",
                                    tmp_path.display(),
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to create temporary state file {}: {}",
                            tmp_path.display(),
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to serialize scheduler state: {}", e);
            }
        }
    }

    /// Mark state as dirty without saving immediately
    fn mark_dirty(&mut self) {
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
            self.dirty = false;
        }
    }

    /// Set the state saver handle for async background persistence
    ///
    /// This should be called after creating the SchedulerRuntime to enable
    /// background state saves. The handle allows the scheduler to notify
    /// the state saver task when state changes occur.
    pub fn set_state_saver(&mut self, saver: StateSaverHandle) {
        self.state_saver = Some(saver);
    }

    /// Load scheduler state from disk
    pub fn load_state(&mut self) {
        let path = self.scheduler.state_path().clone();
        if path.exists() {
            if let Ok(json) = std::fs::read_to_string(&path) {
                match serde_json::from_str::<Scheduler>(&json) {
                    Ok(loaded_scheduler) => {
                        // Apply migrations
                        let migrated_scheduler =
                            match gflow::core::migrations::migrate_state(loaded_scheduler) {
                                Ok(migrated) => migrated,
                                Err(e) => {
                                    tracing::error!(
                                        "State migration failed: {}. Starting with fresh state.",
                                        e
                                    );
                                    tracing::warn!(
                                        "The old state file will be backed up to {}.backup",
                                        path.display()
                                    );
                                    // Try to backup the state file
                                    let backup_path = path.with_extension("json.backup");
                                    if let Err(backup_err) = std::fs::copy(&path, &backup_path) {
                                        tracing::error!(
                                            "Failed to backup state file: {}",
                                            backup_err
                                        );
                                    } else {
                                        tracing::info!(
                                            "Backed up state file to {}",
                                            backup_path.display()
                                        );
                                    }
                                    return; // Exit early, keep default state
                                }
                            };

                        // Update jobs and next_job_id from migrated state
                        let next_id = migrated_scheduler.next_job_id();
                        self.scheduler.jobs = migrated_scheduler.jobs;
                        self.scheduler.set_next_job_id(next_id);

                        // Re-initialize NVML and GPU slots (fresh detection)
                        match Nvml::init() {
                            Ok(nvml) => {
                                self.scheduler.update_gpu_slots(Self::get_gpus(&nvml));
                                self.nvml = Some(nvml);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to initialize NVML during state load: {}. Running without GPU support.", e);
                                self.scheduler.update_gpu_slots(HashMap::new());
                                self.nvml = None;
                            }
                        }

                        // Re-initialize memory tracking with current system values
                        let total_memory_mb = Self::get_total_system_memory_mb();
                        self.scheduler.update_memory(total_memory_mb);
                        self.scheduler.refresh_available_memory();

                        tracing::info!("Successfully loaded state from {}", path.display());
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to deserialize state file {}: {}. Starting with fresh state.",
                            path.display(),
                            e
                        );
                        tracing::warn!(
                            "Your job history may have been lost. The old state file will be backed up to {}.backup",
                            path.display()
                        );
                        // Try to backup the corrupted state file
                        let backup_path = path.with_extension("json.backup");
                        if let Err(backup_err) = std::fs::copy(&path, &backup_path) {
                            tracing::error!(
                                "Failed to backup corrupted state file: {}",
                                backup_err
                            );
                        } else {
                            tracing::info!("Backed up old state file to {}", backup_path.display());
                        }
                    }
                }
            } else {
                tracing::error!("Failed to read state file from {}", path.display());
            }
        } else {
            tracing::info!(
                "No existing state file found at {}, starting fresh",
                path.display()
            );
        }
    }

    /// Refresh GPU slots and available memory
    pub fn refresh(&mut self) {
        self.refresh_gpu_slots();
        self.scheduler.refresh_available_memory();
    }

    /// Refresh GPU slot availability using NVML
    fn refresh_gpu_slots(&mut self) {
        let running_gpu_indices: std::collections::HashSet<u32> = self
            .scheduler
            .jobs
            .values()
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
            .jobs
            .get(&job_id)
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
            let submitted_by = job.submitted_by.clone();
            let (job_id, run_name) = self.scheduler.submit_job(job);
            results.push((job_id, run_name, submitted_by));

            if let Some(job) = self.scheduler.jobs.get(&job_id) {
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

            // Close tmux session if auto_close is enabled
            if should_close_tmux {
                if let Some(name) = run_name {
                    tracing::info!("Auto-closing tmux session '{}' for job {}", name, job_id);
                    if let Err(e) = gflow::tmux::kill_session(&name) {
                        tracing::warn!("Failed to auto-close tmux session '{}': {}", name, e);
                    }
                }
            }

            true
        } else {
            false
        }
    }

    pub async fn fail_job(&mut self, job_id: u32) -> bool {
        let result = self.scheduler.fail_job(job_id);
        if result {
            // Auto-cancel dependent jobs
            let cancelled = self.scheduler.auto_cancel_dependent_jobs(job_id);
            if !cancelled.is_empty() {
                tracing::info!(
                    "Auto-cancelled {} dependent jobs: {:?}",
                    cancelled.len(),
                    cancelled
                );
            }
            self.mark_dirty();
        }
        result
    }

    pub async fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some((was_running, run_name)) = self.scheduler.cancel_job(job_id) {
            // Auto-cancel dependent jobs
            let cancelled = self.scheduler.auto_cancel_dependent_jobs(job_id);
            if !cancelled.is_empty() {
                tracing::info!(
                    "Auto-cancelled {} dependent jobs: {:?}",
                    cancelled.len(),
                    cancelled
                );
            }
            self.mark_dirty();

            // If the job was running, send Ctrl-C to gracefully interrupt it
            if was_running {
                if let Some(name) = run_name {
                    if let Err(e) = gflow::tmux::send_ctrl_c(&name) {
                        tracing::error!("Failed to send C-c to tmux session {}: {}", name, e);
                    }
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
        if let Some(job) = self.scheduler.jobs.get_mut(&job_id) {
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
            .jobs
            .get_mut(&job_id)
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
            job.conda_env = conda_env;
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
            job.depends_on_ids = depends_on_ids;
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
    pub fn jobs(&self) -> &HashMap<u32, Job> {
        &self.scheduler.jobs
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
                        },
                    );
                }
            }
        }
        gpu_slots
    }
}

/// Async scheduling loop with immediate wake-up support
pub async fn run(shared_state: SharedState, notify: SchedulerNotify) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        // Wait for either the 5-second interval OR an immediate wake-up notification
        tokio::select! {
            _ = interval.tick() => {}
            _ = notify.notified() => {
                tracing::debug!("Scheduler triggered by job submission");
            }
        }

        // Step 1: Refresh GPU slots (write lock - very brief)
        {
            let mut state = shared_state.write().await;
            state.refresh();
        }

        // Step 2: Detect zombie jobs (collect data with read lock, check tmux without lock)
        let running_jobs = {
            let state = shared_state.read().await;
            state
                .jobs()
                .values()
                .filter(|j| j.state == JobState::Running)
                .map(|j| (j.id, j.run_name.clone()))
                .collect::<Vec<_>>()
        };

        // Check tmux sessions without holding any lock
        let mut zombie_job_ids = Vec::new();
        for (job_id, run_name) in running_jobs {
            if let Some(rn) = run_name {
                if !gflow::tmux::is_session_exist(&rn) {
                    tracing::warn!("Found zombie job (id: {}), marking as Failed.", job_id);
                    zombie_job_ids.push(job_id);
                }
            }
        }

        // Update zombie jobs (write lock - brief)
        if !zombie_job_ids.is_empty() {
            let mut state = shared_state.write().await;
            for job_id in zombie_job_ids {
                if let Some(job) = state.scheduler.jobs.get_mut(&job_id) {
                    job.state = JobState::Failed;
                    job.finished_at = Some(std::time::SystemTime::now());
                    state.mark_dirty();
                }
            }
        }

        // Step 3: Check for timed-out jobs (read lock to identify, then handle)
        let timed_out_jobs = {
            let state = shared_state.read().await;
            state
                .jobs()
                .values()
                .filter(|job| job.has_exceeded_time_limit())
                .map(|job| {
                    tracing::warn!("Job {} has exceeded time limit, terminating...", job.id);
                    (job.id, job.run_name.clone())
                })
                .collect::<Vec<_>>()
        };

        // Terminate timed-out jobs and update state
        if !timed_out_jobs.is_empty() {
            // Send Ctrl-C to all timed-out jobs first (without lock)
            for (job_id, run_name) in &timed_out_jobs {
                if let Some(run_name) = run_name {
                    if let Err(e) = gflow::tmux::send_ctrl_c(run_name) {
                        tracing::error!("Failed to send C-c to timed-out job {}: {}", job_id, e);
                    }
                }
            }

            // Update all timed-out jobs in memory
            let mut state = shared_state.write().await;
            for (job_id, _) in timed_out_jobs {
                if let Some(job) = state.scheduler.jobs.get_mut(&job_id) {
                    job.try_transition(job_id, JobState::Timeout);

                    // Auto-cancel dependent jobs
                    let cancelled = state.scheduler.auto_cancel_dependent_jobs(job_id);
                    if !cancelled.is_empty() {
                        tracing::info!(
                            "Auto-cancelled {} dependent jobs due to timeout of job {}: {:?}",
                            cancelled.len(),
                            job_id,
                            cancelled
                        );
                    }

                    state.mark_dirty();
                }
            }
        }

        // Step 4a: Prepare jobs for execution (write lock - fast, no I/O)
        let jobs_to_execute = {
            let mut state = shared_state.write().await;
            state.scheduler.prepare_jobs_for_execution()
        }; // Lock released here

        // Step 4b: Execute jobs (NO LOCK - can take seconds due to tmux I/O)
        let execution_results = if !jobs_to_execute.is_empty() {
            // Clone executor Arc - NO LOCK NEEDED!
            let executor = {
                let state = shared_state.read().await;
                state.executor.clone()
            }; // Read lock released immediately

            // Execute jobs without holding ANY lock
            let mut results = Vec::new();
            for job in &jobs_to_execute {
                // Re-check job state before execution (prevents executing cancelled/held jobs)
                let should_execute = {
                    let state = shared_state.read().await;
                    state
                        .jobs()
                        .get(&job.id)
                        .map(|current_job| current_job.state == JobState::Running)
                        .unwrap_or(false)
                };

                if !should_execute {
                    tracing::info!(
                        "Skipping execution of job {} (state changed before execution)",
                        job.id
                    );
                    results.push((
                        job.id,
                        Err("Job state changed before execution".to_string()),
                    ));
                    continue;
                }

                match executor.execute(job) {
                    Ok(_) => {
                        tracing::info!("Executed job {}", job.id);
                        results.push((job.id, Ok(())));
                    }
                    Err(e) => {
                        tracing::error!("Failed to execute job {}: {:?}", job.id, e);
                        results.push((job.id, Err(e.to_string())));
                    }
                }
            }

            results
        } else {
            Vec::new()
        };

        // Step 4c: Handle failures (write lock - brief)
        if !execution_results.is_empty() {
            let mut state = shared_state.write().await;
            state
                .scheduler
                .handle_execution_failures(&execution_results);
            state.mark_dirty();
        }

        // Step 5: Update metrics (read lock for state snapshot)
        #[cfg(feature = "metrics")]
        {
            use gflow::metrics;
            let state = shared_state.read().await;

            // Update job state metrics
            metrics::update_job_state_metrics(state.jobs());

            // Update GPU metrics
            let info = state.info();
            let available_gpus = info.gpus.iter().filter(|g| g.available).count();
            let total_gpus = info.gpus.len();
            metrics::GPU_AVAILABLE
                .with_label_values(&[] as &[&str])
                .set(available_gpus as f64);
            metrics::GPU_TOTAL
                .with_label_values(&[] as &[&str])
                .set(total_gpus as f64);

            // Update memory metrics
            metrics::MEMORY_AVAILABLE_MB
                .with_label_values(&[] as &[&str])
                .set(state.available_memory_mb() as f64);
            metrics::MEMORY_TOTAL_MB
                .with_label_values(&[] as &[&str])
                .set(state.total_memory_mb() as f64);
        }
    }
}
