use crate::events::{EventBus, SchedulerEvent};
use crate::state_saver::StateSaverHandle;
use anyhow::Result;
use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::core::{GPUSlot, GPU, UUID};
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
            // Note: Cascade cancellation is now handled by the cascade_handler event handler
            self.mark_dirty();
        }
        result
    }

    pub async fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some((was_running, run_name)) = self.scheduler.cancel_job(job_id, None) {
            // Note: Cascade cancellation is now handled by the cascade_handler event handler
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

/// Event-driven scheduling loop
pub async fn run_event_driven(shared_state: SharedState, event_bus: Arc<EventBus>) {
    // Spawn all event handlers and monitors
    let handles = vec![
        // Cascade handler - reacts to job failures/cancellations
        tokio::spawn(cascade_handler(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // Scheduler trigger handler with debouncing
        tokio::spawn(scheduler_trigger_handler_with_debounce(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // GPU monitor - polls NVML every 10s
        tokio::spawn(gpu_monitor_task(
            Arc::clone(&shared_state),
            Arc::clone(&event_bus),
        )),
        // Zombie monitor - checks tmux every 30s
        tokio::spawn(zombie_monitor_task(
            Arc::clone(&shared_state),
            Arc::clone(&event_bus),
        )),
        // Zombie handler - reacts to zombie events
        tokio::spawn(zombie_handler_task(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // Timeout monitor - checks time limits every 10s
        tokio::spawn(timeout_monitor_task(
            Arc::clone(&shared_state),
            Arc::clone(&event_bus),
        )),
        // Timeout handler - reacts to timeout events
        tokio::spawn(timeout_handler_task(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // Metrics updater - updates metrics every 5s
        #[cfg(feature = "metrics")]
        tokio::spawn(metrics_updater_task(Arc::clone(&shared_state))),
    ];

    // Wait for all handlers (they run forever)
    for handle in handles {
        if let Err(e) = handle.await {
            tracing::error!("Event handler task panicked: {:?}", e);
        }
    }
}

/// Cascade handler - reacts to job failures/cancellations and triggers cascade cancellation
async fn cascade_handler(
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    loop {
        match events.recv().await {
            Ok(SchedulerEvent::JobCompleted {
                job_id,
                final_state,
                ..
            }) => {
                // Only trigger cascade for failed, cancelled, or timed out jobs
                if matches!(
                    final_state,
                    JobState::Failed | JobState::Cancelled | JobState::Timeout
                ) {
                    let mut state_guard = state.write().await;
                    let cancelled = state_guard.scheduler.auto_cancel_dependent_jobs(job_id);
                    if !cancelled.is_empty() {
                        tracing::info!(
                            "Auto-cancelled {} dependent jobs due to job {} (state: {:?}): {:?}",
                            cancelled.len(),
                            job_id,
                            final_state,
                            cancelled
                        );
                        state_guard.mark_dirty();
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Cascade handler lagged, skipped {} events", skipped);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, cascade handler exiting");
                break;
            }
            _ => {}
        }
    }
}

/// Scheduler trigger handler with debouncing
async fn scheduler_trigger_handler_with_debounce(
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    let mut debounce = tokio::time::interval(Duration::from_millis(100));
    let mut pending_schedule = false;

    loop {
        tokio::select! {
            result = events.recv() => {
                match result {
                    Ok(event) => {
                        match event {
                            SchedulerEvent::JobSubmitted { .. }
                            | SchedulerEvent::JobCompleted { .. }
                            | SchedulerEvent::GpuAvailabilityChanged { .. }
                            | SchedulerEvent::MemoryAvailabilityChanged { .. } => {
                                pending_schedule = true;
                            }
                            _ => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("Scheduler trigger handler lagged, skipped {} events", skipped);
                        pending_schedule = true; // Trigger scheduling to be safe
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("Event bus closed, scheduler trigger handler exiting");
                        break;
                    }
                }
            }
            _ = debounce.tick() => {
                if pending_schedule {
                    trigger_scheduling(&state).await;
                    pending_schedule = false;
                }
            }
        }
    }
}

/// Trigger job scheduling
async fn trigger_scheduling(state: &SharedState) {
    // Step 1: Prepare jobs for execution (write lock - fast, no I/O)
    let jobs_to_execute = {
        let mut state_guard = state.write().await;
        let jobs = state_guard.scheduler.prepare_jobs_for_execution();

        // CRITICAL: Immediately refresh GPU slots after allocation to prevent race condition
        // This ensures that if another scheduling trigger happens before the periodic
        // GPU monitor runs, it will see the updated GPU availability
        if !jobs.is_empty() {
            state_guard.refresh_gpu_slots();
        }

        jobs
    }; // Lock released here

    if jobs_to_execute.is_empty() {
        return;
    }

    // Step 2: Execute jobs (NO LOCK - can take seconds due to tmux I/O)
    let executor = {
        let state_guard = state.read().await;
        state_guard.executor.clone()
    }; // Read lock released immediately

    let mut execution_results = Vec::new();
    for job in &jobs_to_execute {
        // Re-check job state before execution (prevents executing cancelled/held jobs)
        let should_execute = {
            let state_guard = state.read().await;
            state_guard
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
            execution_results.push((
                job.id,
                Err("Job state changed before execution".to_string()),
            ));
            continue;
        }

        match executor.execute(job) {
            Ok(_) => {
                tracing::info!("Executed job {}", job.id);
                execution_results.push((job.id, Ok(())));
            }
            Err(e) => {
                tracing::error!("Failed to execute job {}: {:?}", job.id, e);
                execution_results.push((job.id, Err(e.to_string())));
            }
        }
    }

    // Step 3: Handle failures (write lock - brief)
    if !execution_results.is_empty() {
        let mut state_guard = state.write().await;
        state_guard
            .scheduler
            .handle_execution_failures(&execution_results);
        state_guard.mark_dirty();
    }
}

/// GPU monitor task - polls NVML every 10s and publishes changes
async fn gpu_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    let mut previous_gpu_states: HashMap<u32, bool> = HashMap::new();

    loop {
        interval.tick().await;

        // Refresh GPU slots
        {
            let mut state_guard = state.write().await;
            state_guard.refresh_gpu_slots();
        }

        // Check for changes and publish events
        let state_guard = state.read().await;
        let info = state_guard.info();
        for gpu_info in &info.gpus {
            let previous_available = previous_gpu_states.get(&gpu_info.index).copied();
            if previous_available != Some(gpu_info.available) {
                event_bus.publish(SchedulerEvent::GpuAvailabilityChanged {
                    gpu_index: gpu_info.index,
                    available: gpu_info.available,
                });
                previous_gpu_states.insert(gpu_info.index, gpu_info.available);
            }
        }
    }
}

/// Zombie monitor task - checks tmux sessions every 30s
async fn zombie_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        // Collect running jobs (with read lock)
        let running_jobs = {
            let state_guard = state.read().await;
            state_guard
                .jobs()
                .values()
                .filter(|j| j.state == JobState::Running)
                .map(|j| (j.id, j.run_name.clone()))
                .collect::<Vec<_>>()
        };

        if running_jobs.is_empty() {
            continue;
        }

        // Get all tmux sessions in a single batch call (no lock held)
        let existing_sessions = gflow::tmux::get_all_session_names();

        // Check which jobs are zombies
        for (job_id, run_name) in running_jobs {
            if let Some(rn) = run_name {
                if !existing_sessions.contains(&rn) {
                    tracing::warn!("Found zombie job (id: {}), publishing event", job_id);
                    event_bus.publish(SchedulerEvent::ZombieJobDetected { job_id });
                }
            }
        }
    }
}

/// Zombie handler task - reacts to zombie events and marks jobs as failed
async fn zombie_handler_task(
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    loop {
        match events.recv().await {
            Ok(SchedulerEvent::ZombieJobDetected { job_id }) => {
                let mut state_guard = state.write().await;
                if let Some(job) = state_guard.scheduler.jobs.get_mut(&job_id) {
                    job.state = JobState::Failed;
                    job.finished_at = Some(std::time::SystemTime::now());
                    state_guard.mark_dirty();
                    tracing::info!("Marked zombie job {} as Failed", job_id);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Zombie handler lagged, skipped {} events", skipped);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, zombie handler exiting");
                break;
            }
            _ => {}
        }
    }
}

/// Timeout monitor task - checks time limits every 10s
async fn timeout_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        // Check for timed-out jobs (read lock)
        let timed_out_jobs = {
            let state_guard = state.read().await;
            state_guard
                .jobs()
                .values()
                .filter(|job| job.has_exceeded_time_limit())
                .map(|job| {
                    tracing::warn!("Job {} has exceeded time limit, publishing event", job.id);
                    (job.id, job.run_name.clone())
                })
                .collect::<Vec<_>>()
        };

        // Publish timeout events
        for (job_id, run_name) in timed_out_jobs {
            event_bus.publish(SchedulerEvent::JobTimedOut { job_id, run_name });
        }
    }
}

/// Timeout handler task - reacts to timeout events and terminates jobs
async fn timeout_handler_task(
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    loop {
        match events.recv().await {
            Ok(SchedulerEvent::JobTimedOut { job_id, run_name }) => {
                // Send Ctrl-C to terminate the job (no lock held)
                if let Some(rn) = &run_name {
                    if let Err(e) = gflow::tmux::send_ctrl_c(rn) {
                        tracing::error!("Failed to send C-c to timed-out job {}: {}", job_id, e);
                    }
                }

                // Update job state (write lock)
                let mut state_guard = state.write().await;
                if let Some(job) = state_guard.scheduler.jobs.get_mut(&job_id) {
                    job.try_transition(job_id, JobState::Timeout);

                    // Auto-cancel dependent jobs
                    let cancelled = state_guard.scheduler.auto_cancel_dependent_jobs(job_id);
                    if !cancelled.is_empty() {
                        tracing::info!(
                            "Auto-cancelled {} dependent jobs due to timeout of job {}: {:?}",
                            cancelled.len(),
                            job_id,
                            cancelled
                        );
                    }

                    state_guard.mark_dirty();
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Timeout handler lagged, skipped {} events", skipped);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, timeout handler exiting");
                break;
            }
            _ => {}
        }
    }
}

/// Metrics updater task - updates metrics every 5s
#[cfg(feature = "metrics")]
async fn metrics_updater_task(state: SharedState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        let state_guard = state.read().await;

        // Update job state metrics
        gflow::metrics::update_job_state_metrics(state_guard.jobs());

        // Update GPU metrics
        let info = state_guard.info();
        let available_gpus = info.gpus.iter().filter(|g| g.available).count();
        let total_gpus = info.gpus.len();
        gflow::metrics::GPU_AVAILABLE
            .with_label_values(&[] as &[&str])
            .set(available_gpus as f64);
        gflow::metrics::GPU_TOTAL
            .with_label_values(&[] as &[&str])
            .set(total_gpus as f64);

        // Update memory metrics
        gflow::metrics::MEMORY_AVAILABLE_MB
            .with_label_values(&[] as &[&str])
            .set(state_guard.available_memory_mb() as f64);
        gflow::metrics::MEMORY_TOTAL_MB
            .with_label_values(&[] as &[&str])
            .set(state_guard.total_memory_mb() as f64);
    }
}
