use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::core::{GPUSlot, GPU, UUID};
use nvml_wrapper::Nvml;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;

pub type SharedState = Arc<RwLock<SchedulerRuntime>>;

/// Runtime adapter for Scheduler with system integration
pub struct SchedulerRuntime {
    scheduler: Scheduler,
    nvml: Option<Nvml>,
    dirty: bool, // Tracks if state has changed since last save
}

impl SchedulerRuntime {
    /// Create a new scheduler runtime with state loading and NVML initialization
    pub fn with_state_path(
        executor: Box<dyn Executor>,
        state_path: PathBuf,
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
        let scheduler = SchedulerBuilder::new()
            .with_executor(executor)
            .with_gpu_slots(gpu_slots)
            .with_state_path(state_path)
            .with_total_memory_mb(total_memory_mb)
            .with_allowed_gpu_indices(validated_gpu_indices)
            .build();

        let mut runtime = Self {
            scheduler,
            nvml,
            dirty: false,
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
    }

    /// Clear the dirty flag (used after immediate saves to avoid redundant writes)
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Save state only if dirty flag is set, then clear flag
    async fn save_state_if_dirty(&mut self) {
        if self.dirty {
            self.save_state().await;
            self.dirty = false;
        }
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

    // Job mutation methods with immediate or deferred persistence

    pub async fn submit_job(&mut self, job: Job) -> (u32, String) {
        let result = self.scheduler.submit_job(job);
        // Immediate save to ensure "201 Created" means durable
        self.save_state().await;
        self.clear_dirty(); // Clear dirty flag to avoid redundant save
        result
    }

    /// Submit multiple jobs in a batch with immediate persistence
    /// Returns: Vec<(job_id, run_name, submitted_by)>
    pub async fn submit_jobs(&mut self, jobs: Vec<Job>) -> Vec<(u32, String, String)> {
        let mut results = Vec::with_capacity(jobs.len());

        for job in jobs {
            let submitted_by = job.submitted_by.clone();
            let (job_id, run_name) = self.scheduler.submit_job(job);
            results.push((job_id, run_name, submitted_by));
        }

        // Immediate save to ensure "201 Created" means durable
        self.save_state().await;
        self.clear_dirty(); // Clear dirty flag to avoid redundant save
        results
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
            self.mark_dirty();
        }
        result
    }

    pub async fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some((was_running, run_name)) = self.scheduler.cancel_job(job_id) {
            // If the job was running, send Ctrl-C to gracefully interrupt it
            if was_running {
                if let Some(name) = run_name {
                    if let Err(e) = gflow::tmux::send_ctrl_c(&name) {
                        tracing::error!("Failed to send C-c to tmux session {}: {}", name, e);
                    }
                }
            }
            self.mark_dirty();
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
    }

    // Direct access to jobs for server handlers
    pub fn jobs(&self) -> &HashMap<u32, Job> {
        &self.scheduler.jobs
    }

    // Debug/metrics accessors
    pub fn next_job_id(&self) -> u32 {
        self.scheduler.next_job_id()
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

/// Async scheduling loop
pub async fn run(shared_state: SharedState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;

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
                }
            }
            state.mark_dirty();
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

        // Terminate timed-out jobs without lock, then update state
        for (job_id, run_name) in timed_out_jobs {
            if let Some(run_name) = run_name {
                if let Err(e) = gflow::tmux::send_ctrl_c(&run_name) {
                    tracing::error!("Failed to send C-c to timed-out job {}: {}", job_id, e);
                }
            }

            // Update job state (brief write lock)
            {
                let mut state = shared_state.write().await;
                if let Some(job) = state.scheduler.jobs.get_mut(&job_id) {
                    job.try_transition(job_id, JobState::Timeout);
                }
                state.mark_dirty();
            }
        }

        // Step 4: Schedule and execute new jobs (write lock for scheduling decision)
        {
            let mut state = shared_state.write().await;
            let scheduled = state.scheduler.schedule_jobs();
            // Only mark dirty if jobs were actually scheduled
            if !scheduled.is_empty() {
                state.mark_dirty();
            }
        }

        // Step 5: Flush state if dirty (single save per loop iteration)
        {
            let mut state = shared_state.write().await;
            state.save_state_if_dirty().await;
        }

        // Step 6: Update metrics (read lock for state snapshot)
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
                .with_label_values(&[])
                .set(available_gpus as f64);
            metrics::GPU_TOTAL
                .with_label_values(&[])
                .set(total_gpus as f64);

            // Update memory metrics
            metrics::MEMORY_AVAILABLE_MB
                .with_label_values(&[])
                .set(state.available_memory_mb() as f64);
            metrics::MEMORY_TOTAL_MB
                .with_label_values(&[])
                .set(state.total_memory_mb() as f64);
        }
        // Write lock released here
    }
}
