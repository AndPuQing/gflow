use crate::core::executor::Executor;
use crate::core::info::{GpuInfo, SchedulerInfo};
use crate::core::job::{DependencyMode, Job, JobState, JobStateReason};
use crate::core::reservation::{GpuReservation, ReservationStatus};
use crate::core::{GPUSlot, UUID};
use compact_str::{format_compact, CompactString};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Custom deserializer for jobs field that handles both old HashMap and new Vec formats
fn deserialize_jobs<'de, D>(deserializer: D) -> Result<Vec<Job>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    use serde_json::Value;

    let value = Value::deserialize(deserializer)?;

    match value {
        // New format: array of jobs
        Value::Array(arr) => serde_json::from_value(Value::Array(arr)).map_err(D::Error::custom),
        // Old format: object/map with job IDs as keys
        Value::Object(map) => {
            let mut jobs = Vec::new();
            for (_key, job_value) in map {
                let job: Job = serde_json::from_value(job_value).map_err(D::Error::custom)?;
                jobs.push(job);
            }
            // Sort by job ID to maintain order
            jobs.sort_by_key(|j| j.id);
            Ok(jobs)
        }
        _ => Err(D::Error::custom("jobs must be an array or object")),
    }
}

/// Core scheduler with dependency injection for execution strategy
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Scheduler {
    #[serde(default)]
    pub version: u32,
    #[serde(deserialize_with = "deserialize_jobs")]
    pub jobs: Vec<Job>,
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
    /// Index of job IDs by username for fast dependency resolution
    /// Maps username -> sorted list of job IDs (ascending order)
    #[serde(skip)]
    pub(crate) user_jobs_index: HashMap<CompactString, Vec<u32>>,
    /// Dependency graph for fast circular dependency validation
    /// Maps job_id -> list of dependency job IDs
    #[serde(skip)]
    pub(crate) dependency_graph: HashMap<u32, Vec<u32>>,
    /// GPU reservations
    pub reservations: Vec<GpuReservation>,
    /// Next reservation ID
    pub next_reservation_id: u32,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            version: crate::core::migrations::CURRENT_VERSION,
            jobs: Vec::new(),
            executor: None,
            gpu_slots: HashMap::new(),
            total_memory_mb: 16 * 1024, // Default 16GB
            available_memory_mb: 16 * 1024,
            state_path: PathBuf::from("state.json"),
            next_job_id: 1,
            allowed_gpu_indices: None,
            user_jobs_index: HashMap::new(),
            dependency_graph: HashMap::new(),
            reservations: Vec::new(),
            next_reservation_id: 1,
        }
    }
}

impl Scheduler {
    /// Get a job by ID (job IDs start at 1, so we subtract 1 for the index)
    #[inline]
    pub fn get_job(&self, job_id: u32) -> Option<&Job> {
        if job_id == 0 {
            return None;
        }
        self.jobs.get((job_id - 1) as usize)
    }

    /// Get a mutable job by ID
    #[inline]
    pub fn get_job_mut(&mut self, job_id: u32) -> Option<&mut Job> {
        if job_id == 0 {
            return None;
        }
        self.jobs.get_mut((job_id - 1) as usize)
    }

    /// Check if a job exists
    #[inline]
    pub fn job_exists(&self, job_id: u32) -> bool {
        self.get_job(job_id).is_some()
    }

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
                reason: slot.reason.clone(),
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

        // Generate run_name and keep a copy before moving into job
        let run_name = job
            .run_name
            .take()
            .unwrap_or_else(|| format_compact!("gflow-job-{}", job.id));

        job.state = JobState::Queued;
        job.gpu_ids = None;
        job.run_name = Some(run_name.clone());
        job.submitted_at = Some(std::time::SystemTime::now());

        let job_id = job.id;

        // Update user jobs index
        self.user_jobs_index
            .entry(job.submitted_by.clone())
            .or_default()
            .push(job_id);

        // Update dependency graph only if job has dependencies
        if !job.has_no_dependencies() {
            let deps: Vec<u32> = job.all_dependency_ids().into_vec();
            self.dependency_graph.insert(job_id, deps);
        }

        self.jobs.push(job);
        (job_id, run_name.into())
    }

    /// Finish a job and return whether auto_close_tmux is enabled along with run_name
    /// Returns: Some((should_close_tmux, run_name)) if job exists, None otherwise
    /// Note: Caller is responsible for persisting state and closing tmux if needed
    pub fn finish_job(&mut self, job_id: u32) -> Option<(bool, Option<String>)> {
        if let Some(job) = self.get_job_mut(job_id) {
            let should_close_tmux = job.auto_close_tmux;
            let run_name = job.run_name.as_ref().map(|s| s.to_string());
            job.try_transition(job_id, JobState::Finished);
            Some((should_close_tmux, run_name))
        } else {
            None
        }
    }

    /// Fail a job
    /// Note: Caller is responsible for persisting state after this
    pub fn fail_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.get_job_mut(job_id) {
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
        let job = self.get_job_mut(job_id)?;
        let was_running = job.state == JobState::Running;
        let run_name = job.run_name.as_ref().map(|s| s.to_string());
        job.try_transition(job_id, JobState::Cancelled);
        job.reason = reason.or(Some(JobStateReason::CancelledByUser));
        Some((was_running, run_name))
    }

    /// Put a job on hold
    /// Note: Caller is responsible for persisting state after this
    pub fn hold_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.get_job_mut(job_id) {
            job.try_transition(job_id, JobState::Hold);
            true
        } else {
            false
        }
    }

    /// Release a job from hold back to queue
    /// Note: Caller is responsible for persisting state after this
    pub fn release_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.get_job_mut(job_id) {
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

        // Use index for fast lookup
        let user_jobs = self.user_jobs_index.get(username)?;

        if trimmed == "@" {
            // Most recent submission (last in the list since IDs are ascending)
            return user_jobs.last().copied();
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
                return Some(user_jobs[user_jobs.len() - offset]);
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
        use std::collections::HashSet;

        // Use existing dependency graph instead of rebuilding
        // Run DFS from each dependency to check if it can reach new_job_id
        for &dep_id in dependency_ids {
            if self.has_path_dfs_cached(dep_id, new_job_id, &mut HashSet::new(), dependency_ids) {
                return Err(format!(
                    "Circular dependency detected: Job {} depends on Job {}, \
                     which has a path back to Job {}",
                    new_job_id, dep_id, new_job_id
                ));
            }
        }

        Ok(())
    }

    /// DFS to check if there's a path from start to target using cached graph
    fn has_path_dfs_cached(
        &self,
        current: u32,
        target: u32,
        visited: &mut std::collections::HashSet<u32>,
        new_job_deps: &[u32],
    ) -> bool {
        if current == target {
            return true;
        }

        if visited.contains(&current) {
            return false;
        }

        visited.insert(current);

        // Get neighbors from cached graph, or use new_job_deps if current == target
        let neighbors = if current == target {
            new_job_deps
        } else {
            self.dependency_graph
                .get(&current)
                .map(|v| v.as_slice())
                .unwrap_or(&[])
        };

        for &neighbor in neighbors {
            if self.has_path_dfs_cached(neighbor, target, visited, new_job_deps) {
                return true;
            }
        }

        false
    }

    /// Check if job's dependencies are satisfied
    fn are_dependencies_satisfied(
        job: &Job,
        finished_jobs: &std::collections::HashSet<u32>,
    ) -> bool {
        if job.has_no_dependencies() {
            return true;
        }

        match job.dependency_mode.as_ref().unwrap_or(&DependencyMode::All) {
            DependencyMode::All => job
                .dependency_ids_iter()
                .all(|dep_id| finished_jobs.contains(&dep_id)),
            DependencyMode::Any => job
                .dependency_ids_iter()
                .any(|dep_id| finished_jobs.contains(&dep_id)),
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
                .iter()
                .filter(|j| {
                    j.state == JobState::Queued
                        && j.auto_cancel_on_dependency_failure
                        && j.all_dependency_ids().contains(&current_failed_id)
                })
                .map(|j| j.id)
                .collect();

            for job_id in dependent_job_ids {
                if let Some(job) = self.get_job_mut(job_id) {
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
            .get_job(job_id)
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
                if !self.job_exists(dep_id) {
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
            .iter()
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
        // Update reservation statuses first
        self.update_reservation_statuses();

        let mut job_ids_to_execute = Vec::new();
        let mut available_gpus = self.get_available_gpu_slots();
        let finished_jobs: std::collections::HashSet<u32> = self
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Finished)
            .map(|j| j.id)
            .collect();

        // Collect and sort runnable jobs
        let mut runnable_jobs: Vec<_> = self
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Queued)
            .filter(|j| Self::are_dependencies_satisfied(j, &finished_jobs))
            .map(|j| j.id)
            .collect();

        runnable_jobs.sort_by_key(|job_id| {
            self.get_job(*job_id)
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
            let (
                has_enough_gpus,
                has_enough_memory,
                within_group_limit,
                respects_reservations,
                required_memory,
            ) = if let Some(job) = self.get_job(job_id) {
                let has_enough_gpus = job.gpus as usize <= available_gpus.len();
                let required_memory = job.memory_limit_mb.unwrap_or(0);
                let has_enough_memory = required_memory <= available_memory;

                // Check if job respects active reservations
                let respects_reservations = self.check_job_respects_reservations(
                    &job.submitted_by,
                    job.gpus,
                    &available_gpus,
                );

                // Check group concurrency limit
                let within_group_limit = if let Some(ref group_id) = job.group_id {
                    if let Some(max_concurrent) = job.max_concurrent {
                        // Count running jobs in this group
                        let running_in_group = self
                            .jobs
                            .iter()
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
                    respects_reservations,
                    required_memory,
                )
            } else {
                continue;
            };

            // Now allocate resources if all checks pass
            if has_enough_gpus && has_enough_memory && within_group_limit && respects_reservations {
                // Get user name before mutable borrow
                let job_user = if let Some(job) = self.get_job(job_id) {
                    job.submitted_by.clone()
                } else {
                    continue;
                };

                // Filter out GPUs that are reserved by other users
                let usable_gpus = self.filter_usable_gpus(&job_user, &available_gpus);

                if let Some(job) = self.get_job_mut(job_id) {
                    let gpus_for_job = usable_gpus
                        .into_iter()
                        .take(job.gpus as usize)
                        .collect::<Vec<_>>();

                    // Remove allocated GPUs from available pool
                    available_gpus.retain(|gpu| !gpus_for_job.contains(gpu));

                    job.gpu_ids = Some(gpus_for_job);

                    // Set state to Running and allocate memory
                    job.state = JobState::Running;
                    job.started_at = Some(std::time::SystemTime::now());

                    // Collect job ID instead of cloning immediately
                    job_ids_to_execute.push(job_id);
                }
                // Update memory tracking after releasing the borrow
                available_memory = available_memory.saturating_sub(required_memory);
                self.available_memory_mb = self.available_memory_mb.saturating_sub(required_memory);
            } else if !has_enough_memory {
                if let Some(job) = self.get_job(job_id) {
                    tracing::debug!(
                        "Job {} waiting for memory: needs {}MB, available {}MB",
                        job.id,
                        required_memory,
                        available_memory
                    );
                }
            } else if !respects_reservations {
                if let Some(job) = self.get_job(job_id) {
                    tracing::debug!(
                        "Job {} blocked by active GPU reservations (user: {}, needs {} GPUs)",
                        job.id,
                        job.submitted_by,
                        job.gpus
                    );
                }
            }
        }

        // Clone jobs only once after all allocations are done
        job_ids_to_execute
            .into_iter()
            .filter_map(|id| self.get_job(id).cloned())
            .collect()
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
                if let Some(job) = self.get_job_mut(*job_id) {
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

    /// Rebuild user jobs index from current jobs
    /// Should be called after loading state from disk
    pub fn rebuild_user_jobs_index(&mut self) {
        self.user_jobs_index.clear();
        self.dependency_graph.clear();

        for job in &self.jobs {
            // Rebuild user index
            self.user_jobs_index
                .entry(job.submitted_by.clone())
                .or_default()
                .push(job.id);

            // Rebuild dependency graph
            let deps: Vec<u32> = job.all_dependency_ids().into_vec();
            if !deps.is_empty() {
                self.dependency_graph.insert(job.id, deps);
            }
        }
    }

    /// Get count of jobs by state for monitoring
    pub fn get_job_counts_by_state(&self) -> std::collections::HashMap<JobState, usize> {
        let mut counts = std::collections::HashMap::new();
        for job in &self.jobs {
            *counts.entry(job.state).or_insert(0) += 1;
        }
        counts
    }

    /// Get all jobs submitted by a specific user using the index for O(n) performance
    /// where n is the number of jobs by that user (not total jobs)
    pub fn get_jobs_by_user(&self, username: &str) -> Vec<&Job> {
        if let Some(job_ids) = self.user_jobs_index.get(username) {
            job_ids.iter().filter_map(|&id| self.get_job(id)).collect()
        } else {
            Vec::new()
        }
    }

    // ===== GPU Reservation Methods =====

    /// Create a new GPU reservation
    pub fn create_reservation(
        &mut self,
        user: CompactString,
        gpu_spec: crate::core::reservation::GpuSpec,
        start_time: std::time::SystemTime,
        duration: std::time::Duration,
    ) -> anyhow::Result<u32> {
        use crate::core::conflict;
        use crate::core::reservation::{GpuReservation, ReservationStatus};

        // Validate GPU spec
        let total_gpus = self.gpu_slots_count() as u32;
        let gpu_count = gpu_spec.count();

        if gpu_count == 0 {
            anyhow::bail!("GPU count must be greater than 0");
        }
        if gpu_count > total_gpus {
            anyhow::bail!(
                "Requested {} GPUs but only {} GPUs available",
                gpu_count,
                total_gpus
            );
        }

        // Validate GPU indices if specified
        if let Some(indices) = gpu_spec.indices() {
            for &idx in indices {
                if idx >= total_gpus {
                    anyhow::bail!(
                        "GPU index {} is out of range (available: 0-{})",
                        idx,
                        total_gpus - 1
                    );
                }
            }
        }

        // Validate start time (not in past)
        let now = std::time::SystemTime::now();
        if start_time < now {
            anyhow::bail!("Start time cannot be in the past");
        }

        // Check for conflicts using pure functions
        let end_time = start_time + duration;
        let state = conflict::collect_reservation_state(&self.reservations, start_time, end_time);
        conflict::check_reservation_conflict(&gpu_spec, &state, total_gpus)?;

        // Create reservation
        let id = self.next_reservation_id;
        self.next_reservation_id += 1;

        let reservation = GpuReservation {
            id,
            user,
            gpu_spec,
            start_time,
            duration,
            status: ReservationStatus::Pending,
            created_at: now,
            cancelled_at: None,
        };

        self.reservations.push(reservation);

        // Sort reservations by start_time for efficient queries
        self.reservations.sort_by_key(|r| r.start_time);

        Ok(id)
    }

    /// Get a reservation by ID
    pub fn get_reservation(&self, id: u32) -> Option<&GpuReservation> {
        self.reservations.iter().find(|r| r.id == id)
    }

    /// Get a mutable reservation by ID
    pub fn get_reservation_mut(&mut self, id: u32) -> Option<&mut GpuReservation> {
        self.reservations.iter_mut().find(|r| r.id == id)
    }

    /// Cancel a reservation
    pub fn cancel_reservation(&mut self, id: u32) -> anyhow::Result<()> {
        use crate::core::reservation::ReservationStatus;

        let reservation = self
            .get_reservation_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Reservation {} not found", id))?;

        match reservation.status {
            ReservationStatus::Completed => {
                anyhow::bail!("Cannot cancel completed reservation");
            }
            ReservationStatus::Cancelled => {
                anyhow::bail!("Reservation already cancelled");
            }
            ReservationStatus::Pending | ReservationStatus::Active => {
                reservation.status = ReservationStatus::Cancelled;
                reservation.cancelled_at = Some(std::time::SystemTime::now());
                Ok(())
            }
        }
    }

    /// List reservations with optional filters
    pub fn list_reservations(
        &self,
        user_filter: Option<&str>,
        status_filter: Option<ReservationStatus>,
        active_only: bool,
    ) -> Vec<&GpuReservation> {
        let now = std::time::SystemTime::now();

        self.reservations
            .iter()
            .filter(|r| {
                // User filter
                if let Some(user) = user_filter {
                    if r.user != user {
                        return false;
                    }
                }

                // Status filter
                if let Some(status) = status_filter {
                    if r.status != status {
                        return false;
                    }
                }

                // Active only filter
                if active_only && !r.is_active(now) {
                    return false;
                }

                true
            })
            .collect()
    }

    /// Update reservation statuses based on current time and remove completed/cancelled ones
    pub fn update_reservation_statuses(&mut self) {
        use crate::core::reservation::ReservationStatus;

        let now = std::time::SystemTime::now();

        // Update statuses
        for reservation in &mut self.reservations {
            reservation.update_status(now);
        }

        // Remove completed/cancelled reservations immediately
        self.reservations.retain(|r| {
            matches!(
                r.status,
                ReservationStatus::Pending | ReservationStatus::Active
            )
        });
    }

    /// Get currently active reservations
    pub fn get_active_reservations(&self) -> Vec<&GpuReservation> {
        use crate::core::reservation::ReservationStatus;

        let now = std::time::SystemTime::now();

        self.reservations
            .iter()
            .filter(|r| r.status == ReservationStatus::Active && r.is_active(now))
            .collect()
    }

    /// Check if a job respects active reservations
    /// Returns true if the job can proceed, false if it should be blocked
    fn check_job_respects_reservations(
        &self,
        job_user: &str,
        job_gpu_count: u32,
        available_gpus: &[u32],
    ) -> bool {
        use crate::core::reservation::GpuSpec;
        use std::collections::HashSet;

        let active_reservations = self.get_active_reservations();

        if active_reservations.is_empty() {
            return true; // No active reservations, job can proceed
        }

        let total_gpus = self.gpu_slots_count() as u32;

        // Collect reserved GPU indices by other users
        let mut blocked_indices = HashSet::new();
        let mut user_reserved_count = 0u32;
        let mut user_reserved_indices = Vec::new();
        let mut other_count_reserved = 0u32;

        for reservation in &active_reservations {
            if reservation.user == job_user {
                // This user's reservations
                match &reservation.gpu_spec {
                    GpuSpec::Indices(indices) => {
                        user_reserved_indices.extend(indices.iter().copied());
                    }
                    GpuSpec::Count(count) => {
                        user_reserved_count += count;
                    }
                }
            } else {
                // Other users' reservations
                match &reservation.gpu_spec {
                    GpuSpec::Indices(indices) => {
                        // Block specific GPU indices reserved by others
                        blocked_indices.extend(indices.iter().copied());
                    }
                    GpuSpec::Count(count) => {
                        // Other users' count-based reservations
                        other_count_reserved += count;
                    }
                }
            }
        }

        // If user has index-based reservation, they can use those specific GPUs
        if !user_reserved_indices.is_empty() {
            return job_gpu_count <= user_reserved_indices.len() as u32;
        }

        // If user has count-based reservation, they can use unreserved GPUs
        if user_reserved_count > 0 {
            return job_gpu_count <= user_reserved_count;
        }

        // User has no reservation - can only use GPUs not blocked by index-based reservations
        // and not needed by other count-based reservations
        let available_for_unreserved = total_gpus
            .saturating_sub(blocked_indices.len() as u32)
            .saturating_sub(other_count_reserved);

        // Check that job doesn't exceed available unreserved GPUs
        // and that there are enough physically available GPUs
        let usable_gpus: Vec<u32> = available_gpus
            .iter()
            .filter(|&&gpu| !blocked_indices.contains(&gpu))
            .copied()
            .collect();

        job_gpu_count <= available_for_unreserved && job_gpu_count <= usable_gpus.len() as u32
    }

    /// Filter available GPUs to only include those usable by the given user
    /// considering active reservations
    fn filter_usable_gpus(&self, job_user: &str, available_gpus: &[u32]) -> Vec<u32> {
        use crate::core::reservation::GpuSpec;
        use std::collections::HashSet;

        let active_reservations = self.get_active_reservations();

        if active_reservations.is_empty() {
            return available_gpus.to_vec();
        }

        // Collect reserved GPU indices and user's reservations
        let mut blocked_indices = HashSet::new();
        let mut user_reserved_indices = Vec::new();

        for reservation in &active_reservations {
            if reservation.user == job_user {
                // This user's index-based reservations
                if let GpuSpec::Indices(indices) = &reservation.gpu_spec {
                    user_reserved_indices.extend(indices.iter().copied());
                }
            } else {
                // Other users' index-based reservations block these GPUs
                if let GpuSpec::Indices(indices) = &reservation.gpu_spec {
                    blocked_indices.extend(indices.iter().copied());
                }
            }
        }

        // If user has index-based reservation, prioritize those GPUs
        if !user_reserved_indices.is_empty() {
            return user_reserved_indices
                .into_iter()
                .filter(|gpu| available_gpus.contains(gpu))
                .collect();
        }

        // Otherwise, use any GPU not blocked by others
        available_gpus
            .iter()
            .filter(|&&gpu| !blocked_indices.contains(&gpu))
            .copied()
            .collect()
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
            jobs: Vec::new(),
            executor: self.executor,
            gpu_slots: self.gpu_slots,
            total_memory_mb: self.total_memory_mb,
            available_memory_mb: self.total_memory_mb,
            state_path: self.state_path,
            next_job_id: 1,
            allowed_gpu_indices: self.allowed_gpu_indices,
            user_jobs_index: HashMap::new(),
            dependency_graph: HashMap::new(),
            reservations: Vec::new(),
            next_reservation_id: 1,
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
        assert!(scheduler.job_exists(1));
        assert_eq!(scheduler.get_job(1).unwrap().state, JobState::Queued);
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
        scheduler.get_job_mut(job_id).unwrap().state = JobState::Running;

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
        assert_eq!(scheduler.get_job(job_id).unwrap().state, JobState::Queued);
        let initial_available_memory = scheduler.available_memory_mb;

        // Try to schedule jobs without executor
        let results = scheduler.schedule_jobs();

        // Should return empty results
        assert_eq!(results.len(), 0);

        // Job should STILL be Queued (not stuck in Running)
        assert_eq!(
            scheduler.get_job(job_id).unwrap().state,
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
            scheduler.get_job(job_id).unwrap().gpu_ids,
            None,
            "GPU IDs should not be assigned when no executor is present"
        );

        // started_at should not be set
        assert_eq!(
            scheduler.get_job(job_id).unwrap().started_at,
            None,
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
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().reason,
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
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_c_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().reason,
            Some(JobStateReason::DependencyFailed(job_a_id))
        );
        assert_eq!(
            scheduler.get_job(job_c_id).unwrap().reason,
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
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_c_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_d_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_e_id).unwrap().state,
            JobState::Cancelled
        );
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
        assert_eq!(scheduler.get_job(job_b_id).unwrap().state, JobState::Queued);
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
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(scheduler.get_job(job_c_id).unwrap().state, JobState::Queued);
    }

    #[test]
    fn test_create_reservation_with_indices() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add some GPU slots
        for i in 0..4 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Create reservation with specific GPU indices
        let result = scheduler.create_reservation(
            "alice".into(),
            GpuSpec::Indices(vec![0, 2]),
            start_time,
            duration,
        );

        assert!(result.is_ok());
        let reservation_id = result.unwrap();
        assert_eq!(reservation_id, 1);

        let reservation = scheduler.get_reservation(reservation_id).unwrap();
        assert_eq!(reservation.user, "alice");
        assert_eq!(reservation.gpu_spec, GpuSpec::Indices(vec![0, 2]));
        assert_eq!(reservation.gpu_spec.count(), 2);
    }

    #[test]
    fn test_reservation_conflict_indices_vs_indices() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add GPU slots
        for i in 0..4 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Create first reservation for GPU 0, 1
        scheduler
            .create_reservation(
                "alice".into(),
                GpuSpec::Indices(vec![0, 1]),
                start_time,
                duration,
            )
            .unwrap();

        // Try to create overlapping reservation for GPU 1, 2 (should fail due to GPU 1 conflict)
        let result = scheduler.create_reservation(
            "bob".into(),
            GpuSpec::Indices(vec![1, 2]),
            start_time,
            duration,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("GPU index 1 is already reserved"));

        // Create non-overlapping reservation for GPU 2, 3 (should succeed)
        let result = scheduler.create_reservation(
            "bob".into(),
            GpuSpec::Indices(vec![2, 3]),
            start_time,
            duration,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_reservation_conflict_count_vs_indices() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add 4 GPU slots
        for i in 0..4 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Create index-based reservation for GPU 0, 1
        scheduler
            .create_reservation(
                "alice".into(),
                GpuSpec::Indices(vec![0, 1]),
                start_time,
                duration,
            )
            .unwrap();

        // Try to create count-based reservation for 3 GPUs (should fail - only 2 unreserved GPUs left)
        let result =
            scheduler.create_reservation("bob".into(), GpuSpec::Count(3), start_time, duration);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("conflicts"));

        // Create count-based reservation for 2 GPUs (should succeed - GPUs 2, 3 available)
        let result =
            scheduler.create_reservation("bob".into(), GpuSpec::Count(2), start_time, duration);

        assert!(result.is_ok());
    }

    #[test]
    fn test_reservation_out_of_range_index() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add only 2 GPU slots
        for i in 0..2 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Try to reserve GPU index 3 (out of range)
        let result = scheduler.create_reservation(
            "alice".into(),
            GpuSpec::Indices(vec![0, 3]),
            start_time,
            duration,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("GPU index 3 is out of range"));
    }

    // Property-based tests for GPU allocation invariants
    mod proptests {
        use super::*;
        use crate::core::reservation::GpuSpec;
        use proptest::prelude::*;

        // Helper to create a scheduler with N GPUs
        fn scheduler_with_gpus(n: u32) -> Scheduler {
            let mut scheduler = create_test_scheduler();
            for i in 0..n {
                scheduler.gpu_slots.insert(
                    format!("GPU-{}", i),
                    GPUSlot {
                        index: i,
                        available: true,
                        reason: None,
                    },
                );
            }
            scheduler
        }

        proptest! {
            /// Property: Total allocated GPUs never exceeds system total
            /// Create multiple reservations and verify no over-allocation
            #[test]
            fn prop_no_gpu_overallocation(
                total_gpus in 2u32..8,
                reservation_count in 1usize..5,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                let mut successful_reservations = Vec::new();

                // Try to create multiple reservations
                for i in 0..reservation_count {
                    let gpu_count = (i as u32 % total_gpus) + 1;
                    let result = scheduler.create_reservation(
                        format!("user{}", i).into(),
                        GpuSpec::Count(gpu_count),
                        start_time,
                        duration,
                    );

                    if result.is_ok() {
                        successful_reservations.push(gpu_count);
                    }
                }

                // Verify: sum of successful reservations <= total_gpus
                let total_allocated: u32 = successful_reservations.iter().sum();
                prop_assert!(total_allocated <= total_gpus);
            }

            /// Property: Index-based reservations never have overlapping indices
            #[test]
            fn prop_no_index_overlap(
                total_gpus in 4u32..8,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Create first reservation with indices [0, 1]
                let res1 = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Indices(vec![0, 1]),
                    start_time,
                    duration,
                );
                prop_assert!(res1.is_ok());

                // Try to create second reservation with overlapping index [1, 2]
                let res2 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Indices(vec![1, 2]),
                    start_time,
                    duration,
                );
                // Should fail due to index 1 conflict
                prop_assert!(res2.is_err());

                // Create third reservation with non-overlapping indices [2, 3]
                let res3 = scheduler.create_reservation(
                    "charlie".into(),
                    GpuSpec::Indices(vec![2, 3]),
                    start_time,
                    duration,
                );
                // Should succeed
                prop_assert!(res3.is_ok());
            }

            /// Property: Count-based reservations respect index-based reservations
            #[test]
            fn prop_count_respects_indices(
                total_gpus in 4u32..8,
                reserved_indices_count in 1u32..3,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Create index-based reservation
                let indices: Vec<u32> = (0..reserved_indices_count).collect();
                scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Indices(indices),
                    start_time,
                    duration,
                ).unwrap();

                let available_for_count = total_gpus - reserved_indices_count;

                // Try to reserve exactly the available count (should succeed)
                let res1 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Count(available_for_count),
                    start_time,
                    duration,
                );
                prop_assert!(res1.is_ok());

                // Try to reserve one more (should fail)
                let res2 = scheduler.create_reservation(
                    "charlie".into(),
                    GpuSpec::Count(1),
                    start_time,
                    duration,
                );
                prop_assert!(res2.is_err());
            }

            /// Property: Non-overlapping time ranges never conflict
            #[test]
            fn prop_no_conflict_different_times(
                total_gpus in 2u32..8,
                gpu_count in 1u32..4,
                time_gap in 1u64..1000,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let gpu_count = std::cmp::min(gpu_count, total_gpus);

                let start1 = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration1 = std::time::Duration::from_secs(7200);

                // Create first reservation
                let res1 = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(gpu_count),
                    start1,
                    duration1,
                );
                prop_assert!(res1.is_ok());

                // Create second reservation starting after first ends
                let start2 = start1 + duration1 + std::time::Duration::from_secs(time_gap);
                let duration2 = std::time::Duration::from_secs(3600);

                let res2 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Count(gpu_count),
                    start2,
                    duration2,
                );
                // Should succeed since time ranges don't overlap
                prop_assert!(res2.is_ok());
            }

            /// Property: Cancelling a reservation frees up resources
            #[test]
            fn prop_cancel_frees_resources(
                total_gpus in 2u32..8,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Reserve all GPUs
                let res1_id = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(total_gpus),
                    start_time,
                    duration,
                ).unwrap();

                // Try to create another reservation (should fail)
                let res2 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Count(1),
                    start_time,
                    duration,
                );
                prop_assert!(res2.is_err());

                // Cancel first reservation
                scheduler.cancel_reservation(res1_id).unwrap();

                // Now should be able to create new reservation
                let res3 = scheduler.create_reservation(
                    "charlie".into(),
                    GpuSpec::Count(total_gpus),
                    start_time,
                    duration,
                );
                prop_assert!(res3.is_ok());
            }

            /// Property: Invalid GPU indices are always rejected
            #[test]
            fn prop_reject_invalid_indices(
                total_gpus in 2u32..8,
                invalid_index in 8u32..100,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Try to reserve an out-of-range GPU index
                let result = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Indices(vec![0, invalid_index]),
                    start_time,
                    duration,
                );

                prop_assert!(result.is_err());
                prop_assert!(result.unwrap_err().to_string().contains("out of range"));
            }

            /// Property: Zero GPU count is always rejected
            #[test]
            fn prop_reject_zero_gpus(
                total_gpus in 2u32..8,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                let result = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(0),
                    start_time,
                    duration,
                );

                prop_assert!(result.is_err());
                prop_assert!(result.unwrap_err().to_string().contains("must be greater than 0"));
            }

            /// Property: Requesting more GPUs than available is always rejected
            #[test]
            fn prop_reject_excessive_gpus(
                total_gpus in 2u32..8,
                extra in 1u32..10,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                let excessive_count = total_gpus + extra;
                let result = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(excessive_count),
                    start_time,
                    duration,
                );

                prop_assert!(result.is_err());
            }
        }
    }
}
