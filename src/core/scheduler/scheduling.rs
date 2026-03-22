use super::*;

impl Scheduler {
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
            .job_runtimes
            .iter()
            .filter(|rt| rt.state == JobState::Running)
            .filter_map(|rt| rt.memory_limit_mb)
            .sum();

        self.available_memory_mb = self.total_memory_mb.saturating_sub(memory_used);
    }

    fn current_gpu_occupancy(&self) -> (HashMap<u32, usize>, HashSet<u32>, HashMap<u32, u64>) {
        let mut shared_gpu_occupancy = HashMap::new();
        let mut exclusive_gpu_occupancy = HashSet::new();
        let mut shared_gpu_memory_usage_mb = HashMap::new();

        for rt in self
            .job_runtimes
            .iter()
            .filter(|rt| rt.state == JobState::Running)
        {
            let Some(gpu_ids) = rt.gpu_ids.as_ref() else {
                continue;
            };

            match rt.gpu_sharing_mode {
                GpuSharingMode::Shared => {
                    for &gpu in gpu_ids {
                        *shared_gpu_occupancy.entry(gpu).or_insert(0) += 1;
                        if let Some(limit_mb) = rt.gpu_memory_limit_mb {
                            *shared_gpu_memory_usage_mb.entry(gpu).or_insert(0) += limit_mb;
                        }
                    }
                }
                GpuSharingMode::Exclusive => {
                    for &gpu in gpu_ids {
                        exclusive_gpu_occupancy.insert(gpu);
                    }
                }
            }
        }

        (
            shared_gpu_occupancy,
            exclusive_gpu_occupancy,
            shared_gpu_memory_usage_mb,
        )
    }

    fn set_job_reason(&mut self, job_id: u32, reason: Option<JobStateReason>) {
        if let Some(rt) = self.get_job_runtime_mut(job_id) {
            rt.reason = reason.map(Box::new);
        }
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
        // Recompute host RAM from currently running jobs before making new decisions.
        // Without this, finished/cancelled jobs can leave stale memory pressure behind.
        self.refresh_available_memory();

        let mut job_ids_to_execute = Vec::new();
        let available_gpus = self.get_available_gpu_slots();
        let (mut shared_gpu_occupancy, mut exclusive_gpu_occupancy, mut shared_gpu_memory_usage_mb) =
            self.current_gpu_occupancy();
        let gpu_total_memory_mb: HashMap<u32, u64> = self
            .gpu_slots
            .values()
            .filter_map(|slot| slot.total_memory_mb.map(|total_mb| (slot.index, total_mb)))
            .collect();

        // Build finished jobs set by iterating only runtimes (hot data)
        let finished_jobs: std::collections::HashSet<u32> = self
            .job_runtimes
            .iter()
            .filter(|rt| rt.state == JobState::Finished)
            .map(|rt| rt.id)
            .collect();

        // Collect and sort runnable jobs - iterate only runtimes (hot path)
        let mut runnable_jobs: Vec<_> = self
            .job_runtimes
            .iter()
            .enumerate()
            .filter(|(_, rt)| rt.state == JobState::Queued)
            .filter(|(idx, _rt)| {
                // Access spec only when needed for dependency check
                let spec = &self.job_specs[*idx];
                Self::are_dependencies_satisfied_split(spec, &finished_jobs)
            })
            .map(|(_idx, rt)| rt.id)
            .collect();

        // Sort by priority - only access runtime fields (hot data)
        runnable_jobs.sort_by_key(|job_id| {
            let idx = (*job_id - 1) as usize;
            if let Some(rt) = self.job_runtimes.get(idx) {
                let time_bonus = Self::calculate_time_bonus(&rt.time_limit);
                std::cmp::Reverse((rt.priority, time_bonus, std::cmp::Reverse(rt.id)))
            } else {
                std::cmp::Reverse((0, 0, std::cmp::Reverse(*job_id)))
            }
        });

        // Allocate resources for runnable jobs
        let mut available_memory = self.available_memory_mb;
        for job_id in runnable_jobs {
            let idx = (job_id - 1) as usize;

            // First, do immutable checks using only runtime (hot data)
            let (
                has_enough_memory,
                within_group_limit,
                respects_reservations,
                required_memory,
                job_user,
                requested_gpu_count,
                gpu_sharing_mode,
                requested_gpu_memory_mb,
            ) = if let Some(rt) = self.job_runtimes.get(idx) {
                let required_memory = rt.memory_limit_mb.unwrap_or(0);
                let has_enough_memory = required_memory <= available_memory;

                // Access spec only for submitted_by (needed for reservation check)
                let job_user = self
                    .job_specs
                    .get(idx)
                    .map(|s| s.submitted_by.clone())
                    .unwrap_or_default();

                // Check if job respects active reservations
                let respects_reservations =
                    self.check_job_respects_reservations(&job_user, rt.gpus, &available_gpus);

                // Check group concurrency limit using runtime data only
                let within_group_limit = if let Some(ref group_id) = rt.group_id {
                    if let Some(max_concurrent) = rt.max_concurrent {
                        // Use O(1) index lookup
                        let running_in_group =
                            self.group_running_count.get(group_id).copied().unwrap_or(0);

                        if running_in_group >= max_concurrent {
                            tracing::debug!(
                                "Job {} waiting: group {} has {}/{} running jobs",
                                rt.id,
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
                    has_enough_memory,
                    within_group_limit,
                    respects_reservations,
                    required_memory,
                    job_user,
                    rt.gpus,
                    rt.gpu_sharing_mode,
                    rt.gpu_memory_limit_mb,
                )
            } else {
                continue;
            };

            // Now allocate resources if all checks pass
            if has_enough_memory && within_group_limit && respects_reservations {
                // Filter out GPUs that are reserved by other users
                let mut usable_gpus = self.filter_usable_gpus(&job_user, &available_gpus);
                self.reorder_usable_gpus(job_id, &mut usable_gpus);

                // Enforce sharing compatibility:
                // - Shared jobs can use idle or shared-occupied GPUs, but never exclusive-occupied GPUs.
                // - Exclusive jobs can only use fully idle GPUs.
                let compatible_gpus: Vec<u32> = usable_gpus
                    .into_iter()
                    .filter(|gpu| match gpu_sharing_mode {
                        GpuSharingMode::Shared => {
                            if exclusive_gpu_occupancy.contains(gpu) {
                                false
                            } else if let Some(requested_gpu_memory_mb) = requested_gpu_memory_mb {
                                if let Some(total_gpu_memory_mb) = gpu_total_memory_mb.get(gpu) {
                                    let used_memory_mb =
                                        shared_gpu_memory_usage_mb.get(gpu).copied().unwrap_or(0);
                                    used_memory_mb.saturating_add(requested_gpu_memory_mb)
                                        <= *total_gpu_memory_mb
                                } else {
                                    // If total GPU memory is unknown, skip this check.
                                    true
                                }
                            } else {
                                true
                            }
                        }
                        GpuSharingMode::Exclusive => {
                            !exclusive_gpu_occupancy.contains(gpu)
                                && shared_gpu_occupancy.get(gpu).copied().unwrap_or(0) == 0
                        }
                    })
                    .collect();
                let has_enough_gpus = requested_gpu_count as usize <= compatible_gpus.len();

                if !has_enough_gpus {
                    self.set_job_reason(job_id, Some(JobStateReason::WaitingForGpu));
                    continue;
                }

                let gpus_for_job: GpuIds = compatible_gpus
                    .into_iter()
                    .take(requested_gpu_count as usize)
                    .collect();
                let mut allocated_gpus = None;
                if let Some(rt) = self.job_runtimes.get_mut(idx) {
                    rt.gpu_ids = Some(gpus_for_job.clone());
                    allocated_gpus = Some(gpus_for_job);
                }

                if let Some(ref allocated) = allocated_gpus {
                    match gpu_sharing_mode {
                        GpuSharingMode::Shared => {
                            for &gpu in allocated {
                                *shared_gpu_occupancy.entry(gpu).or_insert(0) += 1;
                                if let Some(requested_gpu_memory_mb) = requested_gpu_memory_mb {
                                    *shared_gpu_memory_usage_mb.entry(gpu).or_insert(0) +=
                                        requested_gpu_memory_mb;
                                }
                            }
                        }
                        GpuSharingMode::Exclusive => {
                            for &gpu in allocated {
                                exclusive_gpu_occupancy.insert(gpu);
                            }
                        }
                    }
                }

                let transitioned = self
                    .transition_job_state(job_id, JobState::Running, None)
                    .unwrap_or(false);

                if transitioned {
                    // Collect job ID instead of cloning immediately
                    job_ids_to_execute.push(job_id);

                    // Update memory tracking after releasing the borrow
                    available_memory = available_memory.saturating_sub(required_memory);
                    self.available_memory_mb =
                        self.available_memory_mb.saturating_sub(required_memory);
                } else {
                    // Roll back provisional GPU allocation if we couldn't transition to Running.
                    if let Some(allocated) = allocated_gpus {
                        match gpu_sharing_mode {
                            GpuSharingMode::Shared => {
                                for gpu in allocated {
                                    if let Some(count) = shared_gpu_occupancy.get_mut(&gpu) {
                                        *count = count.saturating_sub(1);
                                        if *count == 0 {
                                            shared_gpu_occupancy.remove(&gpu);
                                        }
                                    }
                                    if let Some(requested_gpu_memory_mb) = requested_gpu_memory_mb {
                                        if let Some(used_memory_mb) =
                                            shared_gpu_memory_usage_mb.get_mut(&gpu)
                                        {
                                            *used_memory_mb = used_memory_mb
                                                .saturating_sub(requested_gpu_memory_mb);
                                            if *used_memory_mb == 0 {
                                                shared_gpu_memory_usage_mb.remove(&gpu);
                                            }
                                        }
                                    }
                                }
                            }
                            GpuSharingMode::Exclusive => {
                                for gpu in allocated {
                                    exclusive_gpu_occupancy.remove(&gpu);
                                }
                            }
                        }
                    }
                    if let Some(rt) = self.job_runtimes.get_mut(idx) {
                        rt.gpu_ids = None;
                    }
                    self.set_job_reason(job_id, Some(JobStateReason::WaitingForResources));
                }
            } else if !has_enough_memory {
                self.set_job_reason(job_id, Some(JobStateReason::WaitingForMemory));
                if let Some(rt) = self.job_runtimes.get(idx) {
                    tracing::debug!(
                        "Job {} waiting for memory: needs {}MB, available {}MB",
                        rt.id,
                        required_memory,
                        available_memory
                    );
                }
            } else if !within_group_limit {
                self.set_job_reason(job_id, Some(JobStateReason::WaitingForResources));
            } else if !respects_reservations {
                self.set_job_reason(job_id, Some(JobStateReason::WaitingForGpu));
                if let Some(rt) = self.job_runtimes.get(idx) {
                    tracing::debug!(
                        "Job {} blocked by active GPU reservations (user: {}, needs {} GPUs)",
                        rt.id,
                        job_user,
                        rt.gpus
                    );
                }
            }
        }

        // Clone jobs only once after all allocations are done
        job_ids_to_execute
            .into_iter()
            .filter_map(|id| self.get_job(id))
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
                let Some((had_gpus, required_memory)) = (|| {
                    let rt = self.get_job_runtime_mut(*job_id)?;
                    let had_gpus = rt.gpu_ids.take().is_some();
                    let required_memory = rt.memory_limit_mb.unwrap_or(0);
                    Some((had_gpus, required_memory))
                })() else {
                    continue;
                };

                // Keep previous behavior: return `true` (job exists) even if transition isn't valid,
                // and always release resources when they were allocated.
                self.transition_job_state(*job_id, JobState::Failed, None);

                // Return memory if we had allocated GPUs (i.e. we were running).
                if had_gpus {
                    self.available_memory_mb =
                        self.available_memory_mb.saturating_add(required_memory);
                    // Note: GPUs will be returned in next refresh cycle.
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
    pub fn update_gpu_slots(&mut self, new_slots: HashMap<GpuUuid, GPUSlot>) {
        self.gpu_slots = new_slots;
    }

    /// Update total and available memory
    pub fn update_memory(&mut self, total_memory_mb: u64) {
        self.total_memory_mb = total_memory_mb;
        self.refresh_available_memory();
    }

    /// Get a reference to gpu_slots for external access
    pub fn gpu_slots_mut(&mut self) -> &mut HashMap<GpuUuid, GPUSlot> {
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
        self.state_jobs_index.clear();
        self.project_jobs_index.clear();
        self.dependency_graph.clear();
        self.dependents_graph.clear();
        self.group_running_count.clear();

        self.check_invariant();

        for (idx, spec) in self.job_specs.iter().enumerate() {
            let rt = &self.job_runtimes[idx];

            // Rebuild user index.
            self.user_jobs_index
                .entry(spec.submitted_by.clone())
                .or_default()
                .push(rt.id);

            // Rebuild state index.
            self.state_jobs_index
                .entry(rt.state)
                .or_default()
                .push(rt.id);

            // Rebuild project index.
            if let Some(ref project) = spec.project {
                self.project_jobs_index
                    .entry(project.clone())
                    .or_default()
                    .push(rt.id);
            }

            // Rebuild dependency graph.
            if spec.depends_on.is_some() || !spec.depends_on_ids.is_empty() {
                let mut deps: Vec<u32> = spec.depends_on_ids.iter().copied().collect();
                if let Some(dep) = spec.depends_on {
                    if !deps.contains(&dep) {
                        deps.push(dep);
                    }
                }
                self.dependency_graph.insert(rt.id, deps);
                for dep in self
                    .dependency_graph
                    .get(&rt.id)
                    .into_iter()
                    .flatten()
                    .copied()
                {
                    let entry = self.dependents_graph.entry(dep).or_default();
                    match entry.binary_search(&rt.id) {
                        Ok(_) => {}
                        Err(pos) => entry.insert(pos, rt.id),
                    }
                }
            }

            // Rebuild group running count index.
            if rt.state == JobState::Running {
                if let Some(group_id) = rt.group_id {
                    *self.group_running_count.entry(group_id).or_insert(0) += 1;
                }
            }
        }
    }

    /// Get the sorted list of job IDs for a state.
    ///
    /// This is primarily intended for API/query paths to avoid scanning all jobs.
    pub fn job_ids_by_state(&self, state: JobState) -> Option<&[u32]> {
        self.state_jobs_index.get(&state).map(|v| v.as_slice())
    }

    /// Get count of jobs by state for monitoring
    pub fn get_job_counts_by_state(&self) -> std::collections::HashMap<JobState, usize> {
        let mut counts = std::collections::HashMap::new();
        for rt in &self.job_runtimes {
            *counts.entry(rt.state).or_insert(0) += 1;
        }
        counts
    }

    /// Get all jobs submitted by a specific user using the index for O(n) performance
    /// where n is the number of jobs by that user (not total jobs)
    pub fn get_jobs_by_user(&self, username: &str) -> Vec<Job> {
        let Some(job_ids) = self.user_jobs_index.get(username) else {
            return Vec::new();
        };

        job_ids.iter().filter_map(|&id| self.get_job(id)).collect()
    }

    /// Get the sorted list of job IDs submitted by a user.
    ///
    /// This is primarily intended for API/query paths to avoid scanning all jobs.
    pub fn job_ids_by_user(&self, username: &str) -> Option<&[u32]> {
        self.user_jobs_index.get(username).map(|v| v.as_slice())
    }

    // ===== GPU Reservation Methods =====
}
