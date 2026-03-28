mod event_loop;
mod monitors;
mod persistence;
mod serialization;

pub use event_loop::run_event_driven;

use super::state_saver::StateSaverHandle;
use anyhow::{Context, Result};
use compact_str::CompactString;
use gflow::core::executor::Executor;
use gflow::core::gpu::{GPUSlot, GpuUuid};
use gflow::core::info::IgnoredGpuProcess;
use gflow::core::job::{GpuSharingMode, Job, JobSpec, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::tmux::disable_pipe_pane_for_job;
use nvml_wrapper::Nvml;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
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
    projects_config: gflow::config::ProjectsConfig,
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
    ignored_gpu_processes: HashSet<IgnoredGpuProcess>,
}

impl SchedulerRuntime {
    fn retry_lineage_root_id(job: &Job) -> u32 {
        job.redone_from.unwrap_or(job.id)
    }

    fn retry_budget_root_id(&self, job_id: u32) -> u32 {
        let mut current_job_id = job_id;

        while let Some(parent_id) = self
            .scheduler
            .get_job_spec(current_job_id)
            .and_then(|spec| spec.retried_from)
        {
            current_job_id = parent_id;
        }

        current_job_id
    }

    fn retries_used_for_budget_root(&self, root_job_id: u32) -> u32 {
        self.scheduler
            .job_specs()
            .iter()
            .enumerate()
            .filter(|spec| {
                spec.1.retried_from.is_some()
                    && self.retry_budget_root_id((spec.0 + 1) as u32) == root_job_id
            })
            .count() as u32
    }

    fn should_retry_job(&self, job: &Job) -> bool {
        if job.state != JobState::Running {
            return false;
        }

        if job.max_retries == 0 {
            return false;
        }

        let root_job_id = self.retry_budget_root_id(job.id);
        self.retries_used_for_budget_root(root_job_id) < job.max_retries
    }

    fn build_retry_job(&self, original_job: &Job) -> Job {
        let retry_root_id = Self::retry_lineage_root_id(original_job);
        let depends_on_ids = original_job.all_dependency_ids();
        let mut builder = Job::builder();

        if let Some(ref script) = original_job.script {
            builder = builder.script((**script).clone());
        }
        if let Some(ref command) = original_job.command {
            builder = builder.command(command.clone());
        }

        builder = builder.gpus(original_job.gpus);
        builder = builder.gpu_sharing_mode(original_job.gpu_sharing_mode);
        builder = builder.priority(original_job.priority);
        builder = builder.conda_env(original_job.conda_env.as_ref().map(|s| s.to_string()));
        builder = builder.time_limit(original_job.time_limit);
        builder = builder.memory_limit_mb(original_job.memory_limit_mb);
        builder = builder.gpu_memory_limit_mb(original_job.gpu_memory_limit_mb);
        builder = builder.depends_on_ids(depends_on_ids.clone());
        builder = builder.dependency_mode(original_job.dependency_mode);
        builder = builder
            .auto_cancel_on_dependency_failure(original_job.auto_cancel_on_dependency_failure);
        if depends_on_ids.len() == 1 {
            builder = builder.depends_on(Some(depends_on_ids[0]));
        }
        builder = builder.run_dir(original_job.run_dir.clone());
        builder = builder.task_id(original_job.task_id);
        builder = builder.max_retries(original_job.max_retries);
        builder = builder.auto_close_tmux(original_job.auto_close_tmux);
        builder = builder.parameters_compact(original_job.parameters.clone());
        builder = builder.group_id_uuid(original_job.group_id);
        builder = builder.max_concurrent(original_job.max_concurrent);
        builder = builder.project(original_job.project.as_ref().map(|s| s.to_string()));
        builder = builder.notifications(original_job.notifications.clone());
        builder = builder.redone_from(Some(retry_root_id));
        builder = builder.retried_from(Some(original_job.id));
        builder = builder.submitted_by(original_job.submitted_by.to_string());

        builder.build()
    }

    async fn finalize_job_with_retry(
        &mut self,
        job_id: u32,
        final_state: JobState,
    ) -> Option<Option<u32>> {
        let original_job = self.scheduler.get_job(job_id)?;

        if !matches!(final_state, JobState::Failed | JobState::Timeout) {
            return None;
        }

        if original_job.state != JobState::Running {
            return None;
        }

        // Timeouts are only delivered after sending Ctrl-C to the running process.
        // We do not have a reliable "process has actually exited" signal yet, so spawning
        // a retry attempt here could run concurrently with the timed-out payload.
        if final_state == JobState::Failed && self.should_retry_job(&original_job) {
            let retry_job = self.build_retry_job(&original_job);
            match self.submit_job(retry_job).await {
                Ok((new_job_id, _run_name, _stored_job)) => {
                    self.scheduler
                        .retarget_dependents_to_retry(job_id, new_job_id);
                    let transitioned = match final_state {
                        JobState::Failed => self.scheduler.fail_job_without_propagation(job_id),
                        JobState::Timeout => self.scheduler.timeout_job_without_propagation(job_id),
                        _ => false,
                    };
                    if transitioned {
                        self.mark_dirty();
                        return Some(Some(new_job_id));
                    }
                }
                Err(error) => {
                    tracing::error!(
                        job_id,
                        desired_state = %final_state,
                        error = %error,
                        "Automatic retry submission failed; falling back to final terminal state"
                    );
                }
            }
        }

        let transitioned = match final_state {
            JobState::Failed => self.scheduler.fail_job(job_id),
            JobState::Timeout => self.scheduler.timeout_job(job_id),
            _ => false,
        };
        if transitioned {
            self.mark_dirty();
            Some(None)
        } else {
            None
        }
    }

    /// Create a new scheduler runtime with state loading and NVML initialization
    pub fn with_state_path(
        executor: Box<dyn Executor>,
        state_dir: PathBuf,
        allowed_gpu_indices: Option<Vec<u32>>,
        gpu_allocation_strategy: gflow::core::gpu_allocation::GpuAllocationStrategy,
        projects_config: gflow::config::ProjectsConfig,
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
            .with_gpu_allocation_strategy(gpu_allocation_strategy)
            .build();

        let mut runtime = Self {
            scheduler,
            projects_config,
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
            ignored_gpu_processes: HashSet::new(),
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

    fn refresh_gpu_slots(&mut self) {
        let mut running_shared_gpu_indices = HashSet::new();
        let mut running_exclusive_gpu_indices = HashSet::new();

        for rt in self
            .scheduler
            .job_runtimes()
            .iter()
            .filter(|rt| rt.state == JobState::Running)
        {
            let Some(gpu_ids) = rt.gpu_ids.as_ref() else {
                continue;
            };

            match rt.gpu_sharing_mode {
                GpuSharingMode::Shared => {
                    for &gpu in gpu_ids {
                        running_shared_gpu_indices.insert(gpu);
                    }
                }
                GpuSharingMode::Exclusive => {
                    for &gpu in gpu_ids {
                        running_exclusive_gpu_indices.insert(gpu);
                    }
                }
            }
        }

        if let Some(nvml) = &self.nvml {
            let ignored_snapshot = self.ignored_gpu_processes.clone();
            let mut active_ignored = ignored_snapshot.clone();
            if let Ok(device_count) = nvml.device_count() {
                for i in 0..device_count {
                    let Ok(device) = nvml.device_by_index(i) else {
                        tracing::warn!(gpu_index = i, "Failed to query NVML device by index");
                        continue;
                    };
                    let Ok(uuid) = device.uuid() else {
                        tracing::warn!(gpu_index = i, "Failed to query NVML device UUID");
                        continue;
                    };
                    let Some(slot) = self.scheduler.gpu_slots_mut().get_mut(&uuid) else {
                        continue;
                    };

                    let occupied_by_exclusive = running_exclusive_gpu_indices.contains(&slot.index);
                    let occupied_by_shared = running_shared_gpu_indices.contains(&slot.index);
                    let slot_index = slot.index;

                    match device.running_compute_processes() {
                        Ok(processes) => {
                            let mut unmanaged_pids = processes
                                .into_iter()
                                .map(|proc| proc.pid)
                                .collect::<Vec<_>>();
                            unmanaged_pids.sort_unstable();
                            unmanaged_pids.dedup();

                            let ignored_pids: Vec<u32> = unmanaged_pids
                                .iter()
                                .copied()
                                .filter(|pid| {
                                    ignored_snapshot.contains(&IgnoredGpuProcess {
                                        gpu_index: slot_index,
                                        pid: *pid,
                                    })
                                })
                                .collect();

                            active_ignored.retain(|entry| entry.gpu_index != slot_index);
                            for pid in &ignored_pids {
                                active_ignored.insert(IgnoredGpuProcess {
                                    gpu_index: slot_index,
                                    pid: *pid,
                                });
                            }

                            unmanaged_pids.retain(|pid| {
                                !ignored_snapshot.contains(&IgnoredGpuProcess {
                                    gpu_index: slot_index,
                                    pid: *pid,
                                })
                            });
                            let is_free_in_nvml = unmanaged_pids.is_empty();
                            slot.available = if occupied_by_exclusive {
                                false
                            } else if occupied_by_shared {
                                true
                            } else {
                                is_free_in_nvml
                            };

                            if !occupied_by_exclusive && !occupied_by_shared {
                                if !is_free_in_nvml {
                                    slot.reason =
                                        Some(format_unmanaged_process_reason(&unmanaged_pids));
                                } else if !ignored_pids.is_empty() {
                                    slot.reason = Some(format_manual_ignore_reason(
                                        slot_index,
                                        &ignored_pids,
                                    ));
                                } else {
                                    slot.reason = None;
                                }
                            } else {
                                slot.reason = None;
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                gpu_index = slot_index,
                                error = ?error,
                                "Failed to inspect running GPU processes; keeping scheduler conservative"
                            );
                            slot.available = occupied_by_shared;
                            slot.reason = if occupied_by_exclusive || occupied_by_shared {
                                None
                            } else {
                                Some("nvml_query_failed".to_string())
                            };
                        }
                    }
                }
            } else {
                tracing::warn!("Failed to query NVML device count during GPU refresh");
            }
            self.ignored_gpu_processes = active_ignored;
        }
    }

    fn current_compute_processes_on_gpu(&self, gpu_index: u32) -> Result<Vec<u32>> {
        let nvml = self
            .nvml
            .as_ref()
            .context("NVML is unavailable; GPU process inspection is not supported")?;

        if !self.scheduler.has_gpu_index(gpu_index) {
            anyhow::bail!(
                "Invalid GPU index {} (scheduler does not manage that GPU)",
                gpu_index,
            );
        }

        let device = nvml
            .device_by_index(gpu_index)
            .with_context(|| format!("Failed to inspect GPU {}", gpu_index))?;
        let mut pids = device
            .running_compute_processes()
            .with_context(|| format!("Failed to inspect running processes on GPU {}", gpu_index))?
            .into_iter()
            .map(|proc| proc.pid)
            .collect::<Vec<_>>();
        pids.sort_unstable();
        pids.dedup();
        Ok(pids)
    }

    pub fn ignore_gpu_process(&mut self, gpu_index: u32, pid: u32) -> Result<bool> {
        let current_pids = self.current_compute_processes_on_gpu(gpu_index)?;
        if !current_pids.contains(&pid) {
            anyhow::bail!("PID {} is not currently running on GPU {}", pid, gpu_index);
        }

        let inserted = self
            .ignored_gpu_processes
            .insert(IgnoredGpuProcess { gpu_index, pid });
        self.refresh_gpu_slots();
        Ok(inserted)
    }

    pub fn unignore_gpu_process(&mut self, gpu_index: u32, pid: u32) -> bool {
        let removed = self
            .ignored_gpu_processes
            .remove(&IgnoredGpuProcess { gpu_index, pid });
        self.refresh_gpu_slots();
        removed
    }

    pub fn list_ignored_gpu_processes(&self) -> Vec<IgnoredGpuProcess> {
        let mut processes = self
            .ignored_gpu_processes
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        processes.sort_unstable();
        processes
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

    fn normalize_and_validate_project(&self, job: &mut Job) -> Result<()> {
        let normalized =
            gflow::utils::validate_project_policy(job.project.as_deref(), &self.projects_config)?;
        job.project = normalized.map(CompactString::from);
        Ok(())
    }

    fn validate_shared_job_requirements(job: &Job) -> Result<()> {
        if job.gpu_sharing_mode == GpuSharingMode::Shared && job.gpu_memory_limit_mb.is_none() {
            anyhow::bail!(
                "Shared jobs must include a GPU memory limit (--gpu-memory / --max-gpu-mem)."
            );
        }
        Ok(())
    }

    fn current_reserved_run_names(&self) -> HashSet<String> {
        let mut reserved_names: HashSet<String> = self
            .scheduler
            .job_specs()
            .iter()
            .zip(self.scheduler.job_runtimes().iter())
            .filter(|(_, rt)| JobState::ACTIVE.contains(&rt.state))
            .filter_map(|(spec, _)| spec.run_name.as_ref().map(|name| name.to_string()))
            .collect();
        reserved_names.extend(gflow::tmux::get_all_session_names());
        reserved_names
    }

    fn allocate_run_name(
        &self,
        job_id: u32,
        requested: Option<&str>,
        reserved_names: &HashSet<String>,
    ) -> String {
        let default_name = format!("gjob-{job_id}");

        let normalized_requested = requested
            .map(gflow::tmux::normalize_session_name)
            .filter(|name| !name.is_empty());
        let base_name = normalized_requested
            .clone()
            .map(|name| format!("gjob-{job_id}-{name}"))
            .unwrap_or_else(|| default_name.clone());

        if !reserved_names.contains(&base_name) {
            return base_name;
        }

        let suffix_seed = normalized_requested
            .as_ref()
            .map(|_| job_id.to_string())
            .unwrap_or_else(|| "1".to_string());
        let mut counter = 0usize;

        loop {
            let candidate = if counter == 0 {
                format!("{base_name}-{suffix_seed}")
            } else {
                format!("{base_name}-{suffix_seed}-{counter}")
            };
            if !reserved_names.contains(&candidate) {
                return candidate;
            }
            counter += 1;
        }
    }

    fn prepare_run_name(&self, job: &mut Job, job_id: u32, reserved_names: &mut HashSet<String>) {
        let requested = job.run_name.as_ref().map(|name| name.as_str());
        let allocated = self.allocate_run_name(job_id, requested, reserved_names);

        if let Some(requested_name) = requested {
            if requested_name != allocated {
                tracing::info!(
                    requested_run_name = %requested_name,
                    effective_run_name = %allocated,
                    "Adjusted run_name for tmux compatibility"
                );
            }
        }

        reserved_names.insert(allocated.clone());
        job.run_name = Some(CompactString::from(allocated));
    }

    pub async fn submit_job(&mut self, mut job: Job) -> Result<(u32, String, Job)> {
        self.normalize_and_validate_project(&mut job)?;
        Self::validate_shared_job_requirements(&job)?;
        let mut reserved_names = self.current_reserved_run_names();
        self.prepare_run_name(&mut job, self.scheduler.next_job_id(), &mut reserved_names);
        let (job_id, run_name) = self.scheduler.submit_job(job);
        self.mark_dirty();

        let job_clone = self
            .scheduler
            .get_job(job_id)
            .expect("Job should exist after submission");

        Ok((job_id, run_name, job_clone))
    }

    /// Submit multiple jobs in a batch
    pub async fn submit_jobs(
        &mut self,
        jobs: Vec<Job>,
    ) -> Result<(Vec<(u32, String, String)>, Vec<Job>, u32)> {
        let mut reserved_names = self.current_reserved_run_names();
        let mut normalized_jobs = Vec::with_capacity(jobs.len());
        for (next_job_id, mut job) in (self.scheduler.next_job_id()..).zip(jobs) {
            self.normalize_and_validate_project(&mut job)?;
            Self::validate_shared_job_requirements(&job)?;
            self.prepare_run_name(&mut job, next_job_id, &mut reserved_names);
            normalized_jobs.push(job);
        }

        let mut results = Vec::with_capacity(normalized_jobs.len());
        let mut submitted_jobs = Vec::with_capacity(normalized_jobs.len());

        for job in normalized_jobs {
            let submitted_by = job.submitted_by.to_string();
            let (job_id, run_name) = self.scheduler.submit_job(job);
            results.push((job_id, run_name, submitted_by));

            if let Some(job) = self.scheduler.get_job(job_id) {
                submitted_jobs.push(job);
            }
        }

        self.mark_dirty();
        let next_id = self.scheduler.next_job_id();
        Ok((results, submitted_jobs, next_id))
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

    pub async fn fail_job(&mut self, job_id: u32) -> Option<Option<u32>> {
        // Get run_name before modifying state (needed for PipePane cleanup)
        let run_name = self
            .scheduler
            .get_job(job_id)
            .and_then(|j| j.run_name.clone());

        let result = self.finalize_job_with_retry(job_id, JobState::Failed).await;
        if result.is_some() {
            // Disable PipePane to prevent process leaks (keep session alive for user inspection)
            if let Some(name) = run_name {
                disable_pipe_pane_for_job(job_id, &name, false);
            }
        }
        result
    }

    pub async fn explicit_fail_job(&mut self, job_id: u32) -> bool {
        let run_name = self
            .scheduler
            .get_job(job_id)
            .and_then(|j| j.run_name.clone());

        let result = self.scheduler.fail_job(job_id);
        if result {
            self.mark_dirty();
            if let Some(name) = run_name {
                disable_pipe_pane_for_job(job_id, &name, false);
            }
        }
        result
    }

    pub async fn timeout_job(&mut self, job_id: u32) -> Option<Option<u32>> {
        let run_name = self
            .scheduler
            .get_job(job_id)
            .and_then(|j| j.run_name.clone());

        let result = self
            .finalize_job_with_retry(job_id, JobState::Timeout)
            .await;
        if result.is_some() {
            if let Some(name) = run_name {
                disable_pipe_pane_for_job(job_id, &name, false);
            }
        }
        result
    }

    pub async fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some((was_running, run_name)) = self.scheduler.cancel_job(job_id, None) {
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
        let (_spec, rt) = self.scheduler.get_job_parts_mut(job_id)?;
        rt.max_concurrent = Some(max_concurrent);
        self.mark_dirty();
        self.scheduler.get_job(job_id)
    }

    /// Update job parameters
    /// Returns Ok((updated_job, updated_fields)) on success, Err(error_message) on failure
    pub async fn update_job(
        &mut self,
        job_id: u32,
        request: super::server::UpdateJobRequest,
    ) -> Result<(Job, Vec<String>), String> {
        let mut updated_fields = Vec::new();
        let old_deps = self.scheduler.dependency_ids_for_job(job_id);

        // Validate the update first
        let new_deps = request.depends_on_ids.as_deref();
        self.scheduler.validate_job_update(job_id, new_deps)?;

        // Enforce shared-job invariant before mutating state.
        if let Some((_spec, rt)) = self.scheduler.get_job_parts(job_id) {
            if rt.gpu_sharing_mode == GpuSharingMode::Shared
                && matches!(request.gpu_memory_limit_mb, Some(None))
            {
                return Err(
                    "Shared jobs must keep a GPU memory limit (--gpu-memory / --max-gpu-mem)."
                        .to_string(),
                );
            }
        }

        {
            let (spec, rt) = self
                .scheduler
                .get_job_parts_mut(job_id)
                .ok_or_else(|| format!("Job {} not found", job_id))?;

            // Apply updates (spec)
            if let Some(command) = request.command {
                spec.command = Some(CompactString::from(command));
                updated_fields.push("command".to_string());
            }

            if let Some(script) = request.script {
                spec.script = Some(Box::new(script));
                updated_fields.push("script".to_string());
            }

            if let Some(gpus) = request.gpus {
                rt.gpus = gpus;
                updated_fields.push("gpus".to_string());
            }

            if let Some(conda_env) = request.conda_env {
                spec.conda_env = conda_env.map(compact_str::CompactString::from);
                updated_fields.push("conda_env".to_string());
            }

            if let Some(priority) = request.priority {
                rt.priority = priority;
                updated_fields.push("priority".to_string());
            }

            if let Some(parameters) = request.parameters {
                spec.parameters = parameters
                    .into_iter()
                    .map(|(k, v)| (CompactString::from(k), CompactString::from(v)))
                    .collect();
                updated_fields.push("parameters".to_string());
            }

            if let Some(time_limit) = request.time_limit {
                rt.time_limit = time_limit;
                updated_fields.push("time_limit".to_string());
            }

            if let Some(memory_limit_mb) = request.memory_limit_mb {
                rt.memory_limit_mb = memory_limit_mb;
                updated_fields.push("memory_limit_mb".to_string());
            }

            if let Some(gpu_memory_limit_mb) = request.gpu_memory_limit_mb {
                rt.gpu_memory_limit_mb = gpu_memory_limit_mb;
                updated_fields.push("gpu_memory_limit_mb".to_string());
            }

            if let Some(depends_on_ids) = request.depends_on_ids {
                spec.depends_on_ids = depends_on_ids.into();
                updated_fields.push("depends_on_ids".to_string());
            }

            if let Some(dependency_mode) = request.dependency_mode {
                spec.dependency_mode = dependency_mode;
                updated_fields.push("dependency_mode".to_string());
            }

            if let Some(auto_cancel) = request.auto_cancel_on_dependency_failure {
                spec.auto_cancel_on_dependency_failure = auto_cancel;
                updated_fields.push("auto_cancel_on_dependency_failure".to_string());
            }

            if let Some(max_concurrent) = request.max_concurrent {
                rt.max_concurrent = max_concurrent;
                updated_fields.push("max_concurrent".to_string());
            }

            if let Some(max_retries) = request.max_retries {
                spec.max_retries = max_retries.unwrap_or(0);
                updated_fields.push("max_retries".to_string());
            }

            if let Some(notifications) = request.notifications {
                spec.notifications = notifications;
                updated_fields.push("notifications".to_string());
            }
        };

        let dependencies_changed = updated_fields.iter().any(|f| f == "depends_on_ids");
        let affects_ready_queue = updated_fields.iter().any(|f| {
            matches!(
                f.as_str(),
                "depends_on_ids"
                    | "dependency_mode"
                    | "auto_cancel_on_dependency_failure"
                    | "priority"
                    | "time_limit"
            )
        });

        if dependencies_changed {
            let new_deps = self.scheduler.dependency_ids_for_job(job_id);
            self.scheduler
                .replace_job_dependencies(job_id, old_deps, new_deps);
        } else if affects_ready_queue {
            self.scheduler.refresh_job_readiness(job_id);
        }

        // Mark state as dirty for persistence
        self.mark_dirty();

        // Return cloned job and list of updated fields
        let updated_job = self
            .scheduler
            .get_job(job_id)
            .ok_or_else(|| format!("Job {} not found", job_id))?;
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

    pub fn gpu_available(&self, gpu_index: u32) -> Option<bool> {
        self.scheduler
            .info()
            .gpus
            .into_iter()
            .find(|gpu| gpu.index == gpu_index)
            .map(|gpu| gpu.available)
    }

    // Materialize all jobs for server handlers (allocates/clones).
    pub fn jobs(&self) -> Vec<Job> {
        self.scheduler.jobs_as_vec()
    }

    // Get a job by ID (materialized).
    pub fn get_job(&self, job_id: u32) -> Option<Job> {
        self.scheduler.get_job(job_id)
    }

    // Read-only access to hot runtimes for monitors/metrics.
    pub fn job_runtimes(&self) -> &[gflow::core::job::JobRuntime] {
        self.scheduler.job_runtimes()
    }

    // Read-only access to cold specs (used by list APIs to avoid full materialization).
    pub fn job_specs(&self) -> &[JobSpec] {
        self.scheduler.job_specs()
    }

    pub fn job_ids_by_user(&self, username: &str) -> Option<&[u32]> {
        self.scheduler.job_ids_by_user(username)
    }

    pub fn job_ids_by_state(&self, state: gflow::core::job::JobState) -> Option<&[u32]> {
        self.scheduler.job_ids_by_state(state)
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

    fn get_gpus(nvml: &Nvml) -> HashMap<GpuUuid, GPUSlot> {
        let mut gpu_slots = HashMap::new();
        let device_count = nvml.device_count().unwrap_or(0);
        for i in 0..device_count {
            if let Ok(device) = nvml.device_by_index(i) {
                if let Ok(uuid) = device.uuid() {
                    let total_memory_mb = device
                        .memory_info()
                        .ok()
                        .map(|mi| mi.total / (1024_u64 * 1024_u64));
                    gpu_slots.insert(
                        uuid,
                        GPUSlot {
                            available: true,
                            index: i,
                            total_memory_mb,
                            reason: None,
                        },
                    );
                }
            }
        }
        gpu_slots
    }
}

fn format_pid_list(pids: &[u32]) -> String {
    pids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_manual_ignore_reason(gpu_index: u32, ignored_pids: &[u32]) -> String {
    format!(
        "manual_ignore(gpu={},pid={})",
        gpu_index,
        format_pid_list(ignored_pids)
    )
}

fn format_unmanaged_process_reason(unmanaged_pids: &[u32]) -> String {
    format!("unmanaged(pid={})", format_pid_list(unmanaged_pids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::executor::Executor;
    use gflow::core::info::IgnoredGpuProcess;
    use gflow::core::job::{GpuSharingMode, Job, JobState};

    struct NoopExecutor;

    impl Executor for NoopExecutor {
        fn execute(&self, _job: &Job) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn formats_gpu_process_reasons() {
        assert_eq!(
            format_manual_ignore_reason(2, &[1234, 5678]),
            "manual_ignore(gpu=2,pid=1234,5678)"
        );
        assert_eq!(
            format_unmanaged_process_reason(&[222, 333]),
            "unmanaged(pid=222,333)"
        );
    }

    #[test]
    fn list_ignored_gpu_processes_is_sorted() {
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            tempfile::tempdir().unwrap().path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        runtime.ignored_gpu_processes.insert(IgnoredGpuProcess {
            gpu_index: 3,
            pid: 99,
        });
        runtime.ignored_gpu_processes.insert(IgnoredGpuProcess {
            gpu_index: 1,
            pid: 200,
        });
        runtime.ignored_gpu_processes.insert(IgnoredGpuProcess {
            gpu_index: 1,
            pid: 100,
        });

        assert_eq!(
            runtime.list_ignored_gpu_processes(),
            vec![
                IgnoredGpuProcess {
                    gpu_index: 1,
                    pid: 100
                },
                IgnoredGpuProcess {
                    gpu_index: 1,
                    pid: 200
                },
                IgnoredGpuProcess {
                    gpu_index: 3,
                    pid: 99
                },
            ]
        );
    }

    #[tokio::test]
    async fn rejects_whitespace_project_when_project_is_required() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig {
                known_projects: vec![],
                require_project: true,
            },
        )
        .unwrap();

        let job = Job::builder()
            .command("echo test")
            .submitted_by("alice")
            .project(Some("   ".to_string()))
            .build();

        let result = runtime.submit_job(job).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Project is required"));
        assert_eq!(runtime.next_job_id(), 1);
        assert!(runtime.get_job(1).is_none());
    }

    #[tokio::test]
    async fn batch_project_validation_is_all_or_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig {
                known_projects: vec!["alpha".to_string()],
                require_project: true,
            },
        )
        .unwrap();

        let valid_job = Job::builder()
            .command("echo valid")
            .submitted_by("alice")
            .project(Some("alpha".to_string()))
            .build();
        let invalid_job = Job::builder()
            .command("echo invalid")
            .submitted_by("alice")
            .project(Some("unknown".to_string()))
            .build();

        let result = runtime.submit_jobs(vec![valid_job, invalid_job]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown project"));
        assert_eq!(runtime.next_job_id(), 1);
        assert!(runtime.get_job(1).is_none());
    }

    #[tokio::test]
    async fn rejects_shared_job_without_gpu_memory_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let job = Job::builder()
            .command("echo test")
            .submitted_by("alice")
            .shared(true)
            .build();

        let result = runtime.submit_job(job).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Shared jobs must include a GPU memory limit"));
        assert_eq!(runtime.next_job_id(), 1);
        assert!(runtime.get_job(1).is_none());
    }

    #[tokio::test]
    async fn normalizes_custom_run_name_for_tmux_targets() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let job = Job::builder()
            .command("echo test")
            .submitted_by("alice")
            .run_name(Some("train:v1.2".to_string()))
            .build();

        let (job_id, run_name, stored_job) = runtime.submit_job(job).await.unwrap();

        assert_eq!(job_id, 1);
        assert_eq!(run_name, "gjob-1-train_v1_2");
        assert_eq!(stored_job.run_name.as_deref(), Some("gjob-1-train_v1_2"));
    }

    #[tokio::test]
    async fn prefixes_custom_run_names_with_job_id_to_avoid_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let job1 = Job::builder()
            .command("echo first")
            .submitted_by("alice")
            .run_name(Some("demo".to_string()))
            .build();
        let job2 = Job::builder()
            .command("echo second")
            .submitted_by("alice")
            .run_name(Some("demo".to_string()))
            .build();

        let (_, run_name1, _) = runtime.submit_job(job1).await.unwrap();
        let (_, run_name2, _) = runtime.submit_job(job2).await.unwrap();

        assert_eq!(run_name1, "gjob-1-demo");
        assert_eq!(run_name2, "gjob-2-demo");
    }

    #[tokio::test]
    async fn batch_submit_assigns_unique_default_run_names() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let job1 = Job::builder()
            .command("echo first")
            .submitted_by("alice")
            .build();
        let job2 = Job::builder()
            .command("echo second")
            .submitted_by("alice")
            .build();

        let (results, submitted_jobs, next_id) =
            runtime.submit_jobs(vec![job1, job2]).await.unwrap();

        assert_eq!(next_id, 3);
        assert_eq!(results.len(), 2);
        assert_eq!(submitted_jobs.len(), 2);
        assert_eq!(results[0].0, 1);
        assert_eq!(results[0].1, "gjob-1");
        assert_eq!(results[1].0, 2);
        assert_eq!(results[1].1, "gjob-2");
        assert_eq!(submitted_jobs[0].run_name.as_deref(), Some("gjob-1"));
        assert_eq!(submitted_jobs[1].run_name.as_deref(), Some("gjob-2"));
    }

    #[tokio::test]
    async fn batch_submit_assigns_unique_custom_run_names() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let job1 = Job::builder()
            .command("echo first")
            .submitted_by("alice")
            .run_name(Some("demo".to_string()))
            .build();
        let job2 = Job::builder()
            .command("echo second")
            .submitted_by("alice")
            .run_name(Some("demo".to_string()))
            .build();

        let (results, submitted_jobs, next_id) =
            runtime.submit_jobs(vec![job1, job2]).await.unwrap();

        assert_eq!(next_id, 3);
        assert_eq!(results.len(), 2);
        assert_eq!(submitted_jobs.len(), 2);
        assert_eq!(results[0].0, 1);
        assert_eq!(results[0].1, "gjob-1-demo");
        assert_eq!(results[1].0, 2);
        assert_eq!(results[1].1, "gjob-2-demo");
        assert_eq!(submitted_jobs[0].run_name.as_deref(), Some("gjob-1-demo"));
        assert_eq!(submitted_jobs[1].run_name.as_deref(), Some("gjob-2-demo"));
    }

    #[tokio::test]
    async fn rejects_updating_shared_job_to_clear_gpu_memory_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let job = Job::builder()
            .command("echo test")
            .submitted_by("alice")
            .shared(true)
            .gpu_memory_limit_mb(Some(1024))
            .build();
        let (job_id, _run_name, _job) = runtime.submit_job(job).await.unwrap();

        let req = crate::multicall::gflowd::server::UpdateJobRequest {
            command: None,
            script: None,
            gpus: None,
            conda_env: None,
            priority: None,
            parameters: None,
            time_limit: None,
            memory_limit_mb: None,
            gpu_memory_limit_mb: Some(None),
            depends_on_ids: None,
            dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            max_concurrent: None,
            max_retries: None,
            notifications: None,
        };

        let result = runtime.update_job(job_id, req).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Shared jobs must keep a GPU memory limit"));

        let current = runtime.get_job(job_id).unwrap();
        assert_eq!(current.gpu_sharing_mode, GpuSharingMode::Shared);
        assert_eq!(current.gpu_memory_limit_mb, Some(1024));
    }

    #[tokio::test]
    async fn updates_job_notifications() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let job = Job::builder()
            .command("echo test")
            .submitted_by("alice")
            .build();
        let (job_id, _run_name, _job) = runtime.submit_job(job).await.unwrap();

        let req = crate::multicall::gflowd::server::UpdateJobRequest {
            command: None,
            script: None,
            gpus: None,
            conda_env: None,
            priority: None,
            parameters: None,
            time_limit: None,
            memory_limit_mb: None,
            gpu_memory_limit_mb: None,
            depends_on_ids: None,
            dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            max_concurrent: None,
            max_retries: None,
            notifications: Some(gflow::core::job::JobNotifications::normalized(
                vec!["alice@example.com".to_string()],
                vec!["job_failed".to_string()],
            )),
        };

        let (updated, updated_fields) = runtime.update_job(job_id, req).await.unwrap();

        assert_eq!(updated_fields, vec!["notifications".to_string()]);
        assert_eq!(updated.notifications.emails.len(), 1);
        assert_eq!(
            updated.notifications.emails[0].as_str(),
            "alice@example.com"
        );
        assert_eq!(
            updated
                .notifications
                .events
                .iter()
                .map(|event| event.as_str())
                .collect::<Vec<_>>(),
            vec!["job_failed"]
        );
    }

    #[tokio::test]
    async fn fail_job_creates_retry_attempt_and_retargets_queued_dependents() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let root = Job::builder()
            .command("echo root")
            .submitted_by("alice")
            .max_retries(2)
            .build();
        let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

        let child = Job::builder()
            .command("echo child")
            .submitted_by("alice")
            .depends_on(Some(root_id))
            .depends_on_ids(vec![root_id])
            .build();
        let (child_id, _run_name, _job) = runtime.submit_job(child).await.unwrap();

        let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
        assert_eq!(jobs_to_execute.len(), 1);
        assert_eq!(jobs_to_execute[0].id, root_id);
        assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Running);

        let retry_result = runtime.fail_job(root_id).await;
        assert_eq!(retry_result, Some(Some(3)));

        let failed_root = runtime.get_job(root_id).unwrap();
        assert_eq!(failed_root.state, JobState::Failed);

        let retry_job = runtime.get_job(3).unwrap();
        assert_eq!(retry_job.redone_from, Some(root_id));
        assert_eq!(retry_job.max_retries, 2);
        assert_eq!(retry_job.state, JobState::Queued);

        let updated_child = runtime.get_job(child_id).unwrap();
        assert_eq!(updated_child.state, JobState::Queued);
        assert_eq!(updated_child.all_dependency_ids().as_slice(), &[3]);
    }

    #[tokio::test]
    async fn explicit_fail_job_does_not_spawn_retry_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let root = Job::builder()
            .command("echo root")
            .submitted_by("alice")
            .max_retries(2)
            .build();
        let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

        let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
        assert_eq!(jobs_to_execute.len(), 1);
        assert_eq!(jobs_to_execute[0].id, root_id);

        assert!(runtime.explicit_fail_job(root_id).await);
        assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Failed);
        assert!(runtime.get_job(2).is_none());
    }

    #[tokio::test]
    async fn manual_redo_lineage_does_not_consume_automatic_retry_budget() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let root = Job::builder()
            .command("echo root")
            .submitted_by("alice")
            .build();
        let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

        let manual_redo = Job::builder()
            .command("echo redo")
            .submitted_by("alice")
            .redone_from(Some(root_id))
            .max_retries(1)
            .build();
        let (redo_id, _run_name, _job) = runtime.submit_job(manual_redo).await.unwrap();

        let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
        assert_eq!(jobs_to_execute.len(), 2);
        assert!(jobs_to_execute.iter().any(|job| job.id == redo_id));

        let retry_result = runtime.fail_job(redo_id).await;
        assert_eq!(retry_result, Some(Some(3)));

        let retry_job = runtime.get_job(3).unwrap();
        assert_eq!(retry_job.redone_from, Some(root_id));
        assert_eq!(retry_job.retried_from, Some(redo_id));
    }

    #[tokio::test]
    async fn sibling_manual_redos_do_not_share_automatic_retry_budget() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let root = Job::builder()
            .command("echo root")
            .submitted_by("alice")
            .build();
        let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

        let redo_a = Job::builder()
            .command("echo redo a")
            .submitted_by("alice")
            .redone_from(Some(root_id))
            .max_retries(1)
            .build();
        let (redo_a_id, _run_name, _job) = runtime.submit_job(redo_a).await.unwrap();

        let redo_b = Job::builder()
            .command("echo redo b")
            .submitted_by("alice")
            .redone_from(Some(root_id))
            .max_retries(1)
            .build();
        let (redo_b_id, _run_name, _job) = runtime.submit_job(redo_b).await.unwrap();

        let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
        assert!(jobs_to_execute.iter().any(|job| job.id == redo_a_id));
        assert!(jobs_to_execute.iter().any(|job| job.id == redo_b_id));

        assert_eq!(runtime.fail_job(redo_a_id).await, Some(Some(4)));
        assert_eq!(runtime.fail_job(redo_b_id).await, Some(Some(5)));

        let retry_a = runtime.get_job(4).unwrap();
        assert_eq!(retry_a.redone_from, Some(root_id));
        assert_eq!(retry_a.retried_from, Some(redo_a_id));

        let retry_b = runtime.get_job(5).unwrap();
        assert_eq!(retry_b.redone_from, Some(root_id));
        assert_eq!(retry_b.retried_from, Some(redo_b_id));
    }

    #[tokio::test]
    async fn timeout_does_not_spawn_retry_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
        )
        .unwrap();

        let root = Job::builder()
            .command("echo root")
            .submitted_by("alice")
            .max_retries(2)
            .build();
        let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

        let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
        assert_eq!(jobs_to_execute.len(), 1);
        assert_eq!(jobs_to_execute[0].id, root_id);
        assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Running);

        let timeout_result = runtime.timeout_job(root_id).await;
        assert_eq!(timeout_result, Some(None));
        assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Timeout);
        assert!(runtime.get_job(2).is_none());
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
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
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
            gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
            gflow::config::ProjectsConfig::default(),
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
