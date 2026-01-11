use anyhow::Result;
use gflow::core::db::Database;
use gflow::core::db_writer::DatabaseWriter;
use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobEvent, JobState};
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
    pub(crate) db: Database,           // SQLite database for persistence
    pub(crate) writer: DatabaseWriter, // Async database writer with micro-batching
    executor: Arc<dyn Executor>,       // Shared executor for lock-free job execution
}

impl SchedulerRuntime {
    /// Create a new scheduler runtime with state loading and NVML initialization
    pub fn with_state_path(
        executor: Box<dyn Executor>,
        state_dir: PathBuf,
        allowed_gpu_indices: Option<Vec<u32>>,
    ) -> anyhow::Result<Self> {
        // Initialize database
        let db_path = state_dir.join("state.db");
        let json_path = state_dir.join("state.json");

        // Check for migration BEFORE creating database to avoid data loss
        let needs_migration = gflow::core::migrations::needs_migration(&json_path, &db_path);

        tracing::info!("Initializing database at {:?}", db_path);
        let db = Database::new(db_path.clone()).map_err(|e| {
            tracing::error!("Failed to initialize database: {}", e);
            e
        })?;

        // Run migration if needed (checked before database creation)
        if needs_migration {
            tracing::info!("Migrating from state.json to SQLite database...");
            gflow::core::migrations::migrate_json_to_sqlite(&json_path, &db)?;
            tracing::info!("Migration complete. Backup saved to state.json.backup");
        }

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

        // Clone Arc for scheduler (still needed for deprecated schedule_jobs method)
        let executor_for_scheduler: Box<dyn Executor> =
            Box::new(ArcExecutorWrapper(executor_arc.clone()));

        let scheduler = SchedulerBuilder::new()
            .with_executor(executor_for_scheduler)
            .with_gpu_slots(gpu_slots)
            .with_state_path(state_dir)
            .with_total_memory_mb(total_memory_mb)
            .with_allowed_gpu_indices(validated_gpu_indices)
            .build();

        // Initialize async database writer
        let writer = DatabaseWriter::new(db.clone());

        let mut runtime = Self {
            scheduler,
            nvml,
            db,
            writer,
            executor: executor_arc,
        };
        runtime.load_state();
        Ok(runtime)
    }

    /// Save scheduler state to database (bulk operation)
    #[allow(dead_code)]
    pub async fn save_state(&self) {
        // Save all jobs to database
        let jobs: Vec<_> = self.scheduler.jobs.values().cloned().collect();
        if let Err(e) = self.db.update_jobs_batch(&jobs) {
            tracing::error!("Failed to save jobs to database: {}", e);
            return;
        }

        // Save metadata
        let next_id = self.scheduler.next_job_id();
        if let Err(e) = self.db.set_metadata("next_job_id", &next_id.to_string()) {
            tracing::error!("Failed to save next_job_id: {}", e);
        }

        if let Some(ref indices) = self.scheduler.allowed_gpu_indices() {
            if let Ok(json) = serde_json::to_string(indices) {
                let _ = self.db.set_metadata("allowed_gpu_indices", &json);
            }
        }
    }

    /// Load scheduler state from database
    pub fn load_state(&mut self) {
        // Load only active jobs (Queued, Hold, Running) for fast startup
        // Completed/failed jobs are queried from DB on-demand
        match self.db.get_active_jobs() {
            Ok(jobs) => {
                self.scheduler.jobs = jobs;

                // Load next_job_id from metadata
                if let Ok(Some(next_id_str)) = self.db.get_metadata("next_job_id") {
                    if let Ok(next_id) = next_id_str.parse::<u32>() {
                        self.scheduler.set_next_job_id(next_id);
                    }
                }

                // Load allowed_gpu_indices
                if let Ok(Some(gpu_indices_json)) = self.db.get_metadata("allowed_gpu_indices") {
                    if let Ok(indices) = serde_json::from_str(&gpu_indices_json) {
                        self.scheduler.set_allowed_gpu_indices(indices);
                    }
                }

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

                tracing::info!(
                    "Loaded {} active jobs from database (Queued/Hold/Running)",
                    self.scheduler.jobs.len()
                );
            }
            Err(e) => {
                tracing::error!("Failed to load state from database: {}", e);
            }
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

    pub async fn submit_job(&mut self, job: Job) -> (u32, String, Job) {
        let (job_id, run_name) = self.scheduler.submit_job(job);

        // Clone job for database persistence
        let job_clone = self
            .scheduler
            .jobs
            .get(&job_id)
            .cloned()
            .expect("Job should exist after submission");

        (job_id, run_name, job_clone)
    }

    /// Submit multiple jobs in a batch
    /// Returns: (results, jobs_to_persist, next_job_id)
    pub async fn submit_jobs(
        &mut self,
        jobs: Vec<Job>,
    ) -> (Vec<(u32, String, String)>, Vec<Job>, u32) {
        let mut results = Vec::with_capacity(jobs.len());
        let mut db_jobs = Vec::with_capacity(jobs.len());

        for job in jobs {
            let submitted_by = job.submitted_by.clone();
            let (job_id, run_name) = self.scheduler.submit_job(job);
            results.push((job_id, run_name, submitted_by));

            if let Some(job) = self.scheduler.jobs.get(&job_id) {
                db_jobs.push(job.clone());
            }
        }

        let next_id = self.scheduler.next_job_id();
        (results, db_jobs, next_id)
    }

    pub async fn finish_job(&mut self, job_id: u32) -> bool {
        if let Some((should_close_tmux, run_name)) = self.scheduler.finish_job(job_id) {
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
        self.scheduler.fail_job(job_id)
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
            true
        } else {
            false
        }
    }

    pub async fn hold_job(&mut self, job_id: u32) -> bool {
        self.scheduler.hold_job(job_id)
    }

    pub async fn release_job(&mut self, job_id: u32) -> bool {
        self.scheduler.release_job(job_id)
    }

    /// Update max_concurrent for a specific job
    /// Returns the updated job if successful
    pub fn update_job_max_concurrent(&mut self, job_id: u32, max_concurrent: usize) -> Option<Job> {
        if let Some(job) = self.scheduler.jobs.get_mut(&job_id) {
            job.max_concurrent = Some(max_concurrent);
            Some(job.clone())
        } else {
            None
        }
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

    /// Get a job by ID, checking memory first (fast path) then database (for completed jobs)
    pub fn get_job(&self, job_id: u32) -> Result<Option<Job>> {
        // Fast path: check in-memory jobs first (active jobs)
        if let Some(job) = self.scheduler.jobs.get(&job_id) {
            return Ok(Some(job.clone()));
        }

        // Slow path: query database for completed/archived jobs
        self.db.get_job(job_id)
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
            let (dirty_jobs, writer) = {
                let mut state = shared_state.write().await;
                let mut dirty_jobs = Vec::new();
                for job_id in zombie_job_ids {
                    if let Some(job) = state.scheduler.jobs.get_mut(&job_id) {
                        let old_state = job.state;
                        job.state = JobState::Failed;
                        job.finished_at = Some(std::time::SystemTime::now());
                        dirty_jobs.push(job.clone());

                        // Queue event for zombie detection
                        let event = JobEvent::state_transition(
                            job_id,
                            old_state,
                            JobState::Failed,
                            Some("Zombie job detected".to_string()),
                        );
                        state.writer.queue_event(event);
                    }
                }
                (dirty_jobs, state.writer.clone())
            }; // Lock released here

            // Queue updates (non-blocking)
            writer.queue_update_batch(dirty_jobs);
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
            let (dirty_jobs, writer) = {
                let mut state = shared_state.write().await;
                let mut dirty_jobs = Vec::new();
                for (job_id, _) in timed_out_jobs {
                    if let Some(job) = state.scheduler.jobs.get_mut(&job_id) {
                        let old_state = job.state;
                        job.try_transition(job_id, JobState::Timeout);
                        dirty_jobs.push(job.clone());

                        // Queue event for timeout
                        let event = JobEvent::state_transition(
                            job_id,
                            old_state,
                            JobState::Timeout,
                            Some("Job exceeded time limit".to_string()),
                        );
                        state.writer.queue_event(event);
                    }
                }
                (dirty_jobs, state.writer.clone())
            }; // Lock released here

            // Queue updates (non-blocking)
            writer.queue_update_batch(dirty_jobs);
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
                // This handles the race where a job is cancelled between prepare and execute
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
                    // Mark as execution failure so resources are released in Step 4c
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

        // Step 4c: Handle failures and log events (write lock - brief)
        let (dirty_jobs, writer) = if !execution_results.is_empty() {
            let mut state = shared_state.write().await;
            // Handle any execution failures
            state
                .scheduler
                .handle_execution_failures(&execution_results);

            // Collect dirty jobs and log events
            let mut dirty_jobs = Vec::new();
            for (job_id, result) in &execution_results {
                if let Some(job) = state.jobs().get(job_id) {
                    dirty_jobs.push(job.clone());

                    // Log scheduling event
                    if result.is_ok() && job.state == JobState::Running {
                        let event = JobEvent::state_transition(
                            *job_id,
                            JobState::Queued,
                            JobState::Running,
                            None,
                        );
                        state.writer.queue_event(event);

                        // Log GPU assignment if GPUs were assigned
                        if let Some(ref gpu_ids) = job.gpu_ids {
                            if !gpu_ids.is_empty() {
                                let gpu_event = JobEvent::gpu_assignment(*job_id, gpu_ids.clone());
                                state.writer.queue_event(gpu_event);
                            }
                        }
                    }
                }
            }
            (dirty_jobs, state.writer.clone())
        } else {
            (Vec::new(), {
                let state = shared_state.read().await;
                state.writer.clone()
            })
        }; // Lock released here

        // Queue updates (non-blocking)
        if !dirty_jobs.is_empty() {
            writer.queue_update_batch(dirty_jobs);
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
