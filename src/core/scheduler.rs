use crate::core::executor::Executor;
use crate::core::info::{GpuInfo, SchedulerInfo};
use crate::core::job::{Job, JobState};
use crate::core::{GPUSlot, UUID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Core scheduler with dependency injection for execution strategy
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Scheduler {
    pub jobs: HashMap<u32, Job>,
    #[serde(skip)]
    pub(crate) executor: Option<Box<dyn Executor>>,
    #[serde(skip)]
    pub(crate) gpu_slots: HashMap<UUID, GPUSlot>,
    #[serde(skip)]
    pub(crate) total_memory_mb: u64,
    #[serde(skip)]
    pub(crate) available_memory_mb: u64,
    pub(crate) state_path: PathBuf,
    pub(crate) next_job_id: u32,
    /// GPU indices that scheduler is allowed to use (None = all GPUs)
    pub(crate) allowed_gpu_indices: Option<Vec<u32>>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            jobs: HashMap::new(),
            executor: None,
            gpu_slots: HashMap::new(),
            total_memory_mb: 16 * 1024, // Default 16GB
            available_memory_mb: 16 * 1024,
            state_path: PathBuf::from("state.json"),
            next_job_id: 1,
            allowed_gpu_indices: None,
        }
    }
}

impl Scheduler {
    /// Get available GPU slots respecting restrictions
    pub fn get_available_gpu_slots(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self
            .gpu_slots
            .values()
            .filter(|slot| slot.available)
            .map(|slot| slot.index)
            .filter(|&index| {
                // Apply GPU restriction filter
                match &self.allowed_gpu_indices {
                    None => true, // No restriction, all GPUs allowed
                    Some(allowed) => allowed.contains(&index),
                }
            })
            .collect();
        slots.sort_unstable();
        slots
    }

    /// Get scheduler info (GPU status and restrictions)
    pub fn info(&self) -> SchedulerInfo {
        let mut gpus: Vec<GpuInfo> = self
            .gpu_slots
            .iter()
            .map(|(uuid, slot)| GpuInfo {
                uuid: uuid.clone(),
                index: slot.index,
                available: slot.available,
            })
            .collect();
        // Sort by index for stable output
        gpus.sort_by_key(|g| g.index);
        SchedulerInfo {
            gpus,
            allowed_gpu_indices: self.allowed_gpu_indices.clone(),
        }
    }

    /// Get total number of GPU slots
    pub fn gpu_slots_count(&self) -> usize {
        self.gpu_slots.len()
    }

    /// Set GPU restrictions
    pub fn set_allowed_gpu_indices(&mut self, indices: Option<Vec<u32>>) {
        self.allowed_gpu_indices = indices;
    }

    /// Submit a job and return (job_id, run_name)
    /// Note: Caller is responsible for persisting state after this
    pub fn submit_job(&mut self, mut job: Job) -> (u32, String) {
        job.id = self.next_job_id;
        self.next_job_id += 1;
        let job_ = Job {
            state: JobState::Queued,
            gpu_ids: None,
            run_name: job
                .run_name
                .or_else(|| Some(format!("gflow-job-{}", job.id))),
            ..job
        };
        let job_id = job_.id;
        let run_name = job_.run_name.clone().unwrap_or_default();
        self.jobs.insert(job_id, job_);
        (job_id, run_name)
    }

    /// Finish a job and return whether auto_close_tmux is enabled along with run_name
    /// Returns: Some((should_close_tmux, run_name)) if job exists, None otherwise
    /// Note: Caller is responsible for persisting state and closing tmux if needed
    pub fn finish_job(&mut self, job_id: u32) -> Option<(bool, Option<String>)> {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            let should_close_tmux = job.auto_close_tmux;
            let run_name = job.run_name.clone();
            job.try_transition(job_id, JobState::Finished);
            Some((should_close_tmux, run_name))
        } else {
            None
        }
    }

    /// Fail a job
    /// Note: Caller is responsible for persisting state after this
    pub fn fail_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Failed);
            true
        } else {
            false
        }
    }

    /// Cancel a job and return run_name if it needs Ctrl-C (was Running)
    /// Note: Caller is responsible for sending Ctrl-C and persisting state
    pub fn cancel_job(&mut self, job_id: u32) -> Option<(bool, Option<String>)> {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            let was_running = job.state == JobState::Running;
            let run_name = job.run_name.clone();
            job.try_transition(job_id, JobState::Cancelled);
            Some((was_running, run_name))
        } else {
            None
        }
    }

    /// Put a job on hold
    /// Note: Caller is responsible for persisting state after this
    pub fn hold_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Hold);
            true
        } else {
            false
        }
    }

    /// Release a job from hold back to queue
    /// Note: Caller is responsible for persisting state after this
    pub fn release_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Queued);
            true
        } else {
            false
        }
    }

    /// Resolve dependency shorthand to a job ID
    /// Supports formats:
    /// - "@" -> most recent submission by the user
    /// - "@~N" -> Nth most recent submission by the user
    ///   Returns None if shorthand is invalid or history is insufficient
    pub fn resolve_dependency(&self, username: &str, shorthand: &str) -> Option<u32> {
        let trimmed = shorthand.trim();

        if trimmed.is_empty() {
            return None;
        }

        // Get all jobs submitted by the user, sorted by job ID (which corresponds to submission order)
        let mut user_jobs: Vec<_> = self
            .jobs
            .values()
            .filter(|job| job.submitted_by == username)
            .collect();

        // Sort by job ID (ascending) since job IDs are assigned incrementally
        user_jobs.sort_by_key(|job| job.id);

        if trimmed == "@" {
            // Most recent submission
            return user_jobs.last().map(|job| job.id);
        }

        if let Some(offset_str) = trimmed.strip_prefix("@~") {
            if offset_str.is_empty() {
                return None;
            }
            let offset = offset_str.parse::<usize>().ok()?;
            if offset == 0 {
                return None;
            }
            if offset <= user_jobs.len() {
                return user_jobs.get(user_jobs.len() - offset).map(|job| job.id);
            }
        }

        None
    }

    /// Calculate time bonus for scheduling priority
    /// Returns a value between 100-300:
    /// - 100: No time limit (lowest bonus)
    /// - 200-300: Has time limit (shorter jobs get higher bonus)
    pub fn calculate_time_bonus(time_limit: &Option<Duration>) -> u32 {
        match time_limit {
            None => 100, // Jobs without time limits get lowest bonus
            Some(limit) => {
                // Normalize time limit against a 24-hour maximum
                let max_time_secs = 24.0 * 3600.0; // 24 hours in seconds
                let limit_secs = limit.as_secs_f64();
                let normalized = (limit_secs / max_time_secs).min(1.0);

                // Shorter jobs get higher bonus (up to 300)
                // Longer jobs get lower bonus (down to 200)
                // Formula: 200 + (1 - normalized) * 100
                200 + ((1.0 - normalized) * 100.0) as u32
            }
        }
    }

    /// Refresh available memory by calculating memory used by running jobs
    pub fn refresh_available_memory(&mut self) {
        let memory_used: u64 = self
            .jobs
            .values()
            .filter(|j| j.state == JobState::Running)
            .filter_map(|j| j.memory_limit_mb)
            .sum();

        self.available_memory_mb = self.total_memory_mb.saturating_sub(memory_used);
    }

    /// Schedule and execute jobs based on current state
    /// Returns list of (job_id, success) tuples indicating execution results
    /// Note: Caller is responsible for persisting state after scheduling
    pub fn schedule_jobs(&mut self) -> Vec<(u32, Result<(), String>)> {
        if self.executor.is_none() {
            log::warn!("Scheduler has no executor, cannot schedule jobs");
            return Vec::new();
        }

        let mut results = Vec::new();
        let mut available_gpus = self.get_available_gpu_slots();
        let finished_jobs: std::collections::HashSet<u32> = self
            .jobs
            .values()
            .filter(|j| j.state == JobState::Finished)
            .map(|j| j.id)
            .collect();

        // Collect and sort runnable jobs
        let mut runnable_jobs: Vec<_> = self
            .jobs
            .values()
            .filter(|j| j.state == JobState::Queued)
            .filter(|j| {
                if let Some(dependency_id) = j.depends_on {
                    return finished_jobs.contains(&dependency_id);
                }
                true
            })
            .map(|j| j.id)
            .collect();

        runnable_jobs.sort_by_key(|job_id| {
            self.jobs
                .get(job_id)
                .map(|job| {
                    let time_bonus = Self::calculate_time_bonus(&job.time_limit);
                    std::cmp::Reverse((job.priority, time_bonus, std::cmp::Reverse(job.id)))
                })
                .unwrap_or(std::cmp::Reverse((0, 0, std::cmp::Reverse(*job_id))))
        });

        // Execute runnable jobs
        let available_memory = self.available_memory_mb;
        for job_id in runnable_jobs {
            // First, do immutable checks to determine if job can run
            let (has_enough_gpus, has_enough_memory, within_group_limit, required_memory) =
                if let Some(job) = self.jobs.get(&job_id) {
                    let has_enough_gpus = job.gpus as usize <= available_gpus.len();
                    let required_memory = job.memory_limit_mb.unwrap_or(0);
                    let has_enough_memory = required_memory <= available_memory;

                    // Check group concurrency limit
                    let within_group_limit = if let Some(ref group_id) = job.group_id {
                        if let Some(max_concurrent) = job.max_concurrent {
                            // Count running jobs in this group
                            let running_in_group = self
                                .jobs
                                .values()
                                .filter(|j| j.group_id.as_ref() == Some(group_id))
                                .filter(|j| j.state == JobState::Running)
                                .count();

                            if running_in_group >= max_concurrent {
                                log::debug!(
                                    "Job {} waiting: group {} has {}/{} running jobs",
                                    job.id,
                                    group_id,
                                    running_in_group,
                                    max_concurrent
                                );
                                false
                            } else {
                                true
                            }
                        } else {
                            true // No limit specified
                        }
                    } else {
                        true // Not part of a group
                    };

                    (
                        has_enough_gpus,
                        has_enough_memory,
                        within_group_limit,
                        required_memory,
                    )
                } else {
                    continue;
                };

            // Now get mutable borrow if all checks pass
            if has_enough_gpus && has_enough_memory && within_group_limit {
                if let Some(job) = self.jobs.get_mut(&job_id) {
                    let gpus_for_job = available_gpus
                        .drain(..job.gpus as usize)
                        .collect::<Vec<_>>();
                    job.gpu_ids = Some(gpus_for_job);

                    // Execute job using injected executor
                    let executor = self.executor.as_ref().unwrap();
                    match executor.execute(job) {
                        Ok(_) => {
                            job.state = JobState::Running;
                            job.started_at = Some(std::time::SystemTime::now());
                            log::info!("Executing job: {job:?}");
                            // Reserve memory for this job
                            self.available_memory_mb =
                                self.available_memory_mb.saturating_sub(required_memory);
                            results.push((job_id, Ok(())));
                        }
                        Err(e) => {
                            log::error!("Failed to execute job: {e:?}");
                            job.state = JobState::Failed;
                            results.push((job_id, Err(e.to_string())));
                        }
                    }
                }
            } else if !has_enough_memory {
                if let Some(job) = self.jobs.get(&job_id) {
                    log::debug!(
                        "Job {} waiting for memory: needs {}MB, available {}MB",
                        job.id,
                        required_memory,
                        available_memory
                    );
                }
            }
        }

        results
    }

    /// Update GPU slot availability
    pub fn update_gpu_slots(&mut self, new_slots: HashMap<UUID, GPUSlot>) {
        self.gpu_slots = new_slots;
    }

    /// Update total and available memory
    pub fn update_memory(&mut self, total_memory_mb: u64) {
        self.total_memory_mb = total_memory_mb;
        self.refresh_available_memory();
    }

    /// Get a reference to gpu_slots for external access
    pub fn gpu_slots_mut(&mut self) -> &mut HashMap<UUID, GPUSlot> {
        &mut self.gpu_slots
    }

    /// Get the state path
    pub fn state_path(&self) -> &PathBuf {
        &self.state_path
    }

    /// Get the next job ID
    pub fn next_job_id(&self) -> u32 {
        self.next_job_id
    }

    /// Set the next job ID
    pub fn set_next_job_id(&mut self, id: u32) {
        self.next_job_id = id;
    }
}

/// Builder for creating Scheduler instances with dependency injection
pub struct SchedulerBuilder {
    executor: Option<Box<dyn Executor>>,
    gpu_slots: HashMap<UUID, GPUSlot>,
    state_path: PathBuf,
    total_memory_mb: u64,
    allowed_gpu_indices: Option<Vec<u32>>,
}

impl SchedulerBuilder {
    pub fn new() -> Self {
        Self {
            executor: None,
            gpu_slots: HashMap::new(),
            state_path: PathBuf::from("state.json"),
            total_memory_mb: 16 * 1024, // Default 16GB
            allowed_gpu_indices: None,
        }
    }

    pub fn with_executor(mut self, executor: Box<dyn Executor>) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn with_gpu_slots(mut self, slots: HashMap<UUID, GPUSlot>) -> Self {
        self.gpu_slots = slots;
        self
    }

    pub fn with_state_path(mut self, path: PathBuf) -> Self {
        self.state_path = path;
        self
    }

    pub fn with_total_memory_mb(mut self, memory_mb: u64) -> Self {
        self.total_memory_mb = memory_mb;
        self
    }

    pub fn with_allowed_gpu_indices(mut self, indices: Option<Vec<u32>>) -> Self {
        self.allowed_gpu_indices = indices;
        self
    }

    pub fn build(self) -> Scheduler {
        Scheduler {
            jobs: HashMap::new(),
            executor: self.executor,
            gpu_slots: self.gpu_slots,
            total_memory_mb: self.total_memory_mb,
            available_memory_mb: self.total_memory_mb,
            state_path: self.state_path,
            next_job_id: 1,
            allowed_gpu_indices: self.allowed_gpu_indices,
        }
    }
}

impl Default for SchedulerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::job::JobBuilder;
    use std::sync::{Arc, Mutex};

    /// Mock executor for testing
    struct MockExecutor {
        executions: Arc<Mutex<Vec<Job>>>,
        should_fail: bool,
    }

    impl Executor for MockExecutor {
        fn execute(&self, job: &Job) -> anyhow::Result<()> {
            if self.should_fail {
                anyhow::bail!("Mock execution failed")
            } else {
                self.executions.lock().unwrap().push(job.clone());
                Ok(())
            }
        }
    }

    fn create_test_scheduler() -> Scheduler {
        let executor = Box::new(MockExecutor {
            executions: Arc::new(Mutex::new(Vec::new())),
            should_fail: false,
        });

        SchedulerBuilder::new()
            .with_executor(executor)
            .with_state_path(PathBuf::from("/tmp/test.json"))
            .with_total_memory_mb(16 * 1024)
            .build()
    }

    fn create_test_job(username: &str) -> Job {
        JobBuilder::new()
            .submitted_by(username.to_string())
            .run_dir("/tmp")
            .build()
    }

    #[test]
    fn test_submit_job() {
        let mut scheduler = create_test_scheduler();
        let job = create_test_job("test");

        let (job_id, run_name) = scheduler.submit_job(job);
        assert_eq!(job_id, 1);
        assert_eq!(run_name, "gflow-job-1");
        assert!(scheduler.jobs.contains_key(&1));
        assert_eq!(scheduler.jobs[&1].state, JobState::Queued);
    }

    #[test]
    fn test_resolve_dependency_most_recent() {
        let mut scheduler = create_test_scheduler();

        for _i in 0..3 {
            let job = JobBuilder::new()
                .submitted_by("alice")
                .run_dir("/tmp")
                .build();
            scheduler.submit_job(job);
        }

        assert_eq!(scheduler.resolve_dependency("alice", "@"), Some(3));
    }

    #[test]
    fn test_resolve_dependency_offset() {
        let mut scheduler = create_test_scheduler();

        for _i in 0..5 {
            let job = JobBuilder::new()
                .submitted_by("bob")
                .run_dir("/tmp")
                .build();
            scheduler.submit_job(job);
        }

        assert_eq!(scheduler.resolve_dependency("bob", "@~1"), Some(5));
        assert_eq!(scheduler.resolve_dependency("bob", "@~2"), Some(4));
        assert_eq!(scheduler.resolve_dependency("bob", "@~5"), Some(1));
        assert_eq!(scheduler.resolve_dependency("bob", "@~6"), None); // Out of range
    }

    #[test]
    fn test_calculate_time_bonus() {
        // No time limit
        assert_eq!(Scheduler::calculate_time_bonus(&None), 100);

        // 1 minute (very short)
        assert_eq!(
            Scheduler::calculate_time_bonus(&Some(Duration::from_secs(60))),
            299
        );

        // 24 hours (maximum)
        assert_eq!(
            Scheduler::calculate_time_bonus(&Some(Duration::from_secs(24 * 3600))),
            200
        );
    }

    #[test]
    fn test_refresh_available_memory() {
        let mut scheduler = create_test_scheduler();
        let total = scheduler.total_memory_mb;

        // Submit and "run" a job with memory limit
        let job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .memory_limit_mb(Some(1024))
            .build();
        let (job_id, _) = scheduler.submit_job(job);

        // Manually set to running
        scheduler.jobs.get_mut(&job_id).unwrap().state = JobState::Running;

        scheduler.refresh_available_memory();
        assert_eq!(scheduler.available_memory_mb, total - 1024);
    }
}
