use crate::core::executor::Executor;
use crate::core::info::{GpuInfo, SchedulerInfo};
use crate::core::job::{Job, JobState, JobStateReason};
use crate::core::{GPUSlot, UUID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Core scheduler with dependency injection for execution strategy
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Scheduler {
    #[serde(default)]
    pub version: u32,
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
            version: crate::core::migrations::CURRENT_VERSION,
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

    /// Get GPU restrictions
    pub fn allowed_gpu_indices(&self) -> Option<&Vec<u32>> {
        self.allowed_gpu_indices.as_ref()
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
    pub fn cancel_job(
        &mut self,
        job_id: u32,
        reason: Option<JobStateReason>,
    ) -> Option<(bool, Option<String>)> {
        let job = self.jobs.get_mut(&job_id)?;
        let was_running = job.state == JobState::Running;
        let run_name = job.run_name.clone();
        job.try_transition(job_id, JobState::Cancelled);
        job.reason = reason.or(Some(JobStateReason::CancelledByUser));
        Some((was_running, run_name))
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

    /// Detect circular dependencies using DFS
    /// Returns Ok(()) if no cycle, Err with cycle description if found
    pub fn validate_no_circular_dependency(
        &self,
        new_job_id: u32,
        dependency_ids: &[u32],
    ) -> Result<(), String> {
        use std::collections::{HashMap, HashSet};

        // Build adjacency list for existing jobs + new job
        let mut graph: HashMap<u32, Vec<u32>> = HashMap::new();

        // Add existing dependencies
        for (job_id, job) in &self.jobs {
            let deps = if !job.depends_on_ids.is_empty() {
                job.depends_on_ids.clone()
            } else if let Some(dep) = job.depends_on {
                vec![dep]
            } else {
                vec![]
            };

            if !deps.is_empty() {
                graph.insert(*job_id, deps);
            }
        }

        // Add new job's dependencies
        if !dependency_ids.is_empty() {
            graph.insert(new_job_id, dependency_ids.to_vec());
        }

        // Run DFS from each dependency to check if it can reach new_job_id
        for &dep_id in dependency_ids {
            if self.has_path_dfs(&graph, dep_id, new_job_id, &mut HashSet::new()) {
                return Err(format!(
                    "Circular dependency detected: Job {} depends on Job {}, \
                     which has a path back to Job {}",
                    new_job_id, dep_id, new_job_id
                ));
            }
        }

        Ok(())
    }

    /// DFS to check if there's a path from start to target
    fn has_path_dfs(
        &self,
        graph: &std::collections::HashMap<u32, Vec<u32>>,
        current: u32,
        target: u32,
        visited: &mut std::collections::HashSet<u32>,
    ) -> bool {
        if current == target {
            return true;
        }

        if visited.contains(&current) {
            return false;
        }

        visited.insert(current);

        if let Some(neighbors) = graph.get(&current) {
            for &neighbor in neighbors {
                if self.has_path_dfs(graph, neighbor, target, visited) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if job's dependencies are satisfied
    fn are_dependencies_satisfied(
        &self,
        job: &Job,
        finished_jobs: &std::collections::HashSet<u32>,
        _failed_jobs: &std::collections::HashSet<u32>,
    ) -> bool {
        use crate::core::job::DependencyMode;

        let all_deps = job.all_dependency_ids();

        if all_deps.is_empty() {
            return true; // No dependencies
        }

        // Check based on dependency mode
        match job.dependency_mode.as_ref().unwrap_or(&DependencyMode::All) {
            DependencyMode::All => {
                // ALL dependencies must be finished
                all_deps.iter().all(|dep_id| finished_jobs.contains(dep_id))
            }
            DependencyMode::Any => {
                // At least ONE dependency must be finished
                all_deps.iter().any(|dep_id| finished_jobs.contains(dep_id))
            }
        }
    }

    /// Find and cancel jobs that depend on a failed job (recursively)
    /// Returns list of cancelled job IDs
    pub fn auto_cancel_dependent_jobs(&mut self, failed_job_id: u32) -> Vec<u32> {
        let mut all_cancelled_jobs = Vec::new();
        let mut jobs_to_process = vec![failed_job_id];

        // Process jobs in waves: cancel direct dependents, then their dependents, etc.
        while let Some(current_failed_id) = jobs_to_process.pop() {
            let dependent_job_ids: Vec<u32> = self
                .jobs
                .values()
                .filter(|j| {
                    j.state == JobState::Queued
                        && j.auto_cancel_on_dependency_failure
                        && j.all_dependency_ids().contains(&current_failed_id)
                })
                .map(|j| j.id)
                .collect();

            for job_id in dependent_job_ids {
                if let Some(job) = self.jobs.get_mut(&job_id) {
                    if job.try_transition(job_id, JobState::Cancelled) {
                        job.reason = Some(JobStateReason::DependencyFailed(current_failed_id));
                        tracing::info!(
                            "Auto-cancelled job {} due to failed dependency {}",
                            job_id,
                            current_failed_id
                        );
                        all_cancelled_jobs.push(job_id);
                        // Add this cancelled job to the queue to check its dependents
                        jobs_to_process.push(job_id);
                    }
                }
            }
        }

        all_cancelled_jobs
    }

    /// Validate that a job can be updated
    /// Returns Ok(()) if update is valid, Err(String) with error message otherwise
    pub fn validate_job_update(&self, job_id: u32, new_deps: Option<&[u32]>) -> Result<(), String> {
        // Check if job exists
        let job = self
            .jobs
            .get(&job_id)
            .ok_or_else(|| format!("Job {} not found", job_id))?;

        // Check if job is in updatable state (Queued or Hold)
        if job.state != JobState::Queued && job.state != JobState::Hold {
            return Err(format!(
                "Job {} is in state '{}' and cannot be updated. Only queued or held jobs can be updated.",
                job_id, job.state
            ));
        }

        // If dependencies are being updated, validate them
        if let Some(deps) = new_deps {
            // Check that all dependency IDs exist
            for &dep_id in deps {
                if !self.jobs.contains_key(&dep_id) {
                    return Err(format!("Dependency job {} does not exist", dep_id));
                }
            }

            // Check for circular dependencies
            self.validate_no_circular_dependency(job_id, deps)?;
        }

        Ok(())
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

    /// Prepare jobs for execution by allocating resources and marking them as Running
    ///
    /// # Warning
    /// This method **mutates scheduler state** by:
    /// - Transitioning jobs from Queued to Running
    /// - Allocating GPU and memory resources
    /// - Setting started_at timestamps
    ///
    /// **IMPORTANT**: You MUST either:
    /// 1. Execute the returned jobs (via executor or `execute_jobs_no_lock`)
    /// 2. Handle failures (via `handle_execution_failures`) if execution fails
    ///
    /// Failure to execute will leave jobs stuck in Running state with resources allocated.
    ///
    /// # Returns
    /// Vector of jobs ready to execute with resources already allocated
    ///
    /// # Example
    /// ```ignore
    /// let jobs = scheduler.prepare_jobs_for_execution();
    /// let results = scheduler.execute_jobs_no_lock(&jobs);
    /// scheduler.handle_execution_failures(&results);
    /// ```
    pub fn prepare_jobs_for_execution(&mut self) -> Vec<Job> {
        let mut jobs_to_execute = Vec::new();
        let mut available_gpus = self.get_available_gpu_slots();
        let finished_jobs: std::collections::HashSet<u32> = self
            .jobs
            .values()
            .filter(|j| j.state == JobState::Finished)
            .map(|j| j.id)
            .collect();

        let failed_jobs: std::collections::HashSet<u32> = self
            .jobs
            .values()
            .filter(|j| {
                matches!(
                    j.state,
                    JobState::Failed | JobState::Cancelled | JobState::Timeout
                )
            })
            .map(|j| j.id)
            .collect();

        // Collect and sort runnable jobs
        let mut runnable_jobs: Vec<_> = self
            .jobs
            .values()
            .filter(|j| j.state == JobState::Queued)
            .filter(|j| {
                // Check if dependencies are satisfied
                self.are_dependencies_satisfied(j, &finished_jobs, &failed_jobs)
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

        // Allocate resources for runnable jobs
        let mut available_memory = self.available_memory_mb;
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
                                tracing::debug!(
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

            // Now allocate resources if all checks pass
            if has_enough_gpus && has_enough_memory && within_group_limit {
                if let Some(job) = self.jobs.get_mut(&job_id) {
                    let gpus_for_job = available_gpus
                        .drain(..job.gpus as usize)
                        .collect::<Vec<_>>();
                    job.gpu_ids = Some(gpus_for_job);

                    // Set state to Running and allocate memory
                    job.state = JobState::Running;
                    job.started_at = Some(std::time::SystemTime::now());

                    available_memory = available_memory.saturating_sub(required_memory);
                    self.available_memory_mb =
                        self.available_memory_mb.saturating_sub(required_memory);

                    // Add to execution list
                    jobs_to_execute.push(job.clone());
                }
            } else if !has_enough_memory {
                if let Some(job) = self.jobs.get(&job_id) {
                    tracing::debug!(
                        "Job {} waiting for memory: needs {}MB, available {}MB",
                        job.id,
                        required_memory,
                        available_memory
                    );
                }
            }
        }

        jobs_to_execute
    }

    /// Phase 2: Execute jobs (call executor - can be done WITHOUT holding lock)
    /// This is separated so the caller can release locks before doing I/O
    /// Returns execution results WITHOUT modifying state
    pub fn execute_jobs_no_lock(&self, jobs: &[Job]) -> Vec<(u32, Result<(), String>)> {
        if self.executor.is_none() {
            tracing::warn!("Scheduler has no executor, cannot execute jobs");
            return Vec::new();
        }

        let executor = self.executor.as_ref().unwrap();
        let mut results = Vec::new();

        for job in jobs {
            match executor.execute(job) {
                Ok(_) => {
                    tracing::info!("Executing job: {job:?}");
                    results.push((job.id, Ok(())));
                }
                Err(e) => {
                    tracing::error!("Failed to execute job {}: {e:?}", job.id);
                    results.push((job.id, Err(e.to_string())));
                }
            }
        }

        results
    }

    /// Handle execution failures by marking jobs as failed and releasing resources
    /// Should be called WITH a lock after execute_jobs_no_lock
    pub fn handle_execution_failures(&mut self, results: &[(u32, Result<(), String>)]) {
        for (job_id, result) in results {
            if result.is_err() {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.try_transition(*job_id, JobState::Failed);
                    // Return GPUs and memory
                    if let Some(_gpu_ids) = job.gpu_ids.take() {
                        let required_memory = job.memory_limit_mb.unwrap_or(0);
                        self.available_memory_mb =
                            self.available_memory_mb.saturating_add(required_memory);
                        // Note: GPUs will be returned in next refresh cycle
                    }
                }
            }
        }
    }

    /// Legacy method for backward compatibility - calls both phases
    #[deprecated(
        note = "Use prepare_jobs_for_execution + execute_jobs_no_lock for better performance"
    )]
    pub fn schedule_jobs(&mut self) -> Vec<(u32, Result<(), String>)> {
        // Guard: Check executor exists before mutating state
        if self.executor.is_none() {
            tracing::warn!("Scheduler has no executor, cannot schedule jobs");
            return Vec::new();
        }

        let jobs_to_execute = self.prepare_jobs_for_execution();
        let results = self.execute_jobs_no_lock(&jobs_to_execute);
        self.handle_execution_failures(&results);
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

    /// Get total memory in MB
    pub fn total_memory_mb(&self) -> u64 {
        self.total_memory_mb
    }

    /// Get available memory in MB
    pub fn available_memory_mb(&self) -> u64 {
        self.available_memory_mb
    }

    /// Set the next job ID
    pub fn set_next_job_id(&mut self, id: u32) {
        self.next_job_id = id;
    }

    /// Clean up completed jobs from memory that finished more than `retention_hours` ago
    ///
    /// This removes jobs in final states (Finished, Failed, Cancelled, Timeout) from
    /// the in-memory HashMap to prevent unbounded memory growth. Jobs are still
    /// persisted in the database and can be queried via slow-path queries.
    ///
    /// # Arguments
    /// * `retention_hours` - Keep jobs in memory for this many hours after completion
    ///
    /// # Returns
    /// Number of jobs removed from memory
    ///
    /// # Note
    /// Jobs with active dependencies (other jobs depending on them) are NOT removed
    /// to maintain dependency resolution functionality.
    pub fn cleanup_completed_jobs(&mut self, retention_hours: u64) -> usize {
        use std::time::{SystemTime, UNIX_EPOCH};

        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(retention_hours * 3600);

        // Single-pass algorithm: collect jobs_with_dependents and identify removable jobs simultaneously
        let mut jobs_with_dependents = std::collections::HashSet::new();
        let mut jobs_to_remove = Vec::new();

        for (&job_id, job) in self.jobs.iter() {
            // Track dependencies
            if let Some(depends_on) = job.depends_on {
                jobs_with_dependents.insert(depends_on);
            }

            // Check if job is removable (must be final, old enough)
            if job.state.is_final() {
                if let Some(finished_at) = job.finished_at {
                    if let Ok(duration) = finished_at.duration_since(UNIX_EPOCH) {
                        if duration.as_secs() < cutoff_time {
                            jobs_to_remove.push(job_id);
                        }
                    }
                }
            }
        }

        // Filter out jobs that have active dependents
        jobs_to_remove.retain(|job_id| !jobs_with_dependents.contains(job_id));

        let removed_count = jobs_to_remove.len();

        // Remove jobs from memory using drain_filter pattern for efficiency
        for job_id in jobs_to_remove {
            self.jobs.remove(&job_id);
        }

        if removed_count > 0 {
            tracing::info!(
                "Cleaned up {} completed jobs from memory (retention: {}h)",
                removed_count,
                retention_hours
            );
        }

        removed_count
    }

    /// Get count of jobs by state for monitoring
    pub fn get_job_counts_by_state(&self) -> std::collections::HashMap<JobState, usize> {
        let mut counts = std::collections::HashMap::new();
        for job in self.jobs.values() {
            *counts.entry(job.state).or_insert(0) += 1;
        }
        counts
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
            version: crate::core::migrations::CURRENT_VERSION,
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

    #[test]
    #[allow(deprecated)]
    fn test_schedule_jobs_without_executor_does_not_mutate_state() {
        // Create scheduler WITHOUT executor (simulating misconfiguration)
        let mut scheduler = SchedulerBuilder::new()
            .with_state_path(PathBuf::from("/tmp/test.json"))
            .with_total_memory_mb(16 * 1024)
            .build();

        // Submit a job
        let job = create_test_job("test");
        let (job_id, _) = scheduler.submit_job(job);

        // Verify job is Queued
        assert_eq!(scheduler.jobs[&job_id].state, JobState::Queued);
        let initial_available_memory = scheduler.available_memory_mb;

        // Try to schedule jobs without executor
        let results = scheduler.schedule_jobs();

        // Should return empty results
        assert_eq!(results.len(), 0);

        // Job should STILL be Queued (not stuck in Running)
        assert_eq!(
            scheduler.jobs[&job_id].state,
            JobState::Queued,
            "Job should remain Queued when no executor is present"
        );

        // Memory should not be allocated
        assert_eq!(
            scheduler.available_memory_mb, initial_available_memory,
            "Memory should not be allocated when no executor is present"
        );

        // GPU IDs should not be assigned
        assert_eq!(
            scheduler.jobs[&job_id].gpu_ids, None,
            "GPU IDs should not be assigned when no executor is present"
        );

        // started_at should not be set
        assert_eq!(
            scheduler.jobs[&job_id].started_at, None,
            "started_at should not be set when no executor is present"
        );
    }

    #[test]
    fn test_auto_cancel_direct_dependent() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A with auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        // Fail job A
        scheduler.fail_job(job_a_id);

        // Auto-cancel dependent jobs
        let cancelled = scheduler.auto_cancel_dependent_jobs(job_a_id);

        // Job B should be cancelled
        assert_eq!(cancelled.len(), 1);
        assert!(cancelled.contains(&job_b_id));
        assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Cancelled);
        assert_eq!(
            scheduler.jobs[&job_b_id].reason,
            Some(JobStateReason::DependencyFailed(job_a_id))
        );
    }

    #[test]
    fn test_auto_cancel_transitive_dependencies() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A with auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        // Create job C that depends on B with auto_cancel enabled
        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        // Fail job A
        scheduler.fail_job(job_a_id);

        // Auto-cancel dependent jobs (should cancel both B and C)
        let cancelled = scheduler.auto_cancel_dependent_jobs(job_a_id);

        // Both B and C should be cancelled
        assert_eq!(cancelled.len(), 2);
        assert!(cancelled.contains(&job_b_id));
        assert!(cancelled.contains(&job_c_id));
        assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Cancelled);
        assert_eq!(scheduler.jobs[&job_c_id].state, JobState::Cancelled);
        assert_eq!(
            scheduler.jobs[&job_b_id].reason,
            Some(JobStateReason::DependencyFailed(job_a_id))
        );
        assert_eq!(
            scheduler.jobs[&job_c_id].reason,
            Some(JobStateReason::DependencyFailed(job_b_id))
        );
    }

    #[test]
    fn test_auto_cancel_deep_chain() {
        let mut scheduler = create_test_scheduler();

        // Create a chain: A -> B -> C -> D -> E
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        let job_d = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_c_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_d_id, _) = scheduler.submit_job(job_d);

        let job_e = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_d_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_e_id, _) = scheduler.submit_job(job_e);

        // Fail job A
        scheduler.fail_job(job_a_id);

        // Auto-cancel dependent jobs (should cancel B, C, D, E)
        let cancelled = scheduler.auto_cancel_dependent_jobs(job_a_id);

        // All downstream jobs should be cancelled
        assert_eq!(cancelled.len(), 4);
        assert!(cancelled.contains(&job_b_id));
        assert!(cancelled.contains(&job_c_id));
        assert!(cancelled.contains(&job_d_id));
        assert!(cancelled.contains(&job_e_id));
        assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Cancelled);
        assert_eq!(scheduler.jobs[&job_c_id].state, JobState::Cancelled);
        assert_eq!(scheduler.jobs[&job_d_id].state, JobState::Cancelled);
        assert_eq!(scheduler.jobs[&job_e_id].state, JobState::Cancelled);
    }

    #[test]
    fn test_auto_cancel_respects_flag() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A but WITHOUT auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(false)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        // Fail job A
        scheduler.fail_job(job_a_id);

        // Auto-cancel dependent jobs
        let cancelled = scheduler.auto_cancel_dependent_jobs(job_a_id);

        // Job B should NOT be cancelled
        assert_eq!(cancelled.len(), 0);
        assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Queued);
    }

    #[test]
    fn test_auto_cancel_mixed_flags() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A with auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        // Create job C that depends on B WITHOUT auto_cancel enabled
        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id])
            .auto_cancel_on_dependency_failure(false)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        // Fail job A
        scheduler.fail_job(job_a_id);

        // Auto-cancel dependent jobs
        let cancelled = scheduler.auto_cancel_dependent_jobs(job_a_id);

        // Only B should be cancelled, not C (because C has auto_cancel disabled)
        assert_eq!(cancelled.len(), 1);
        assert!(cancelled.contains(&job_b_id));
        assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Cancelled);
        assert_eq!(scheduler.jobs[&job_c_id].state, JobState::Queued);
    }
}
