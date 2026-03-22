use super::*;

impl Scheduler {
    fn replace_job_dependencies_index(&mut self, job_id: u32, deps: Vec<u32>) {
        if let Some(old_deps) = self.dependency_graph.remove(&job_id) {
            for dep in old_deps {
                if let Some(dependents) = self.dependents_graph.get_mut(&dep) {
                    if let Ok(pos) = dependents.binary_search(&job_id) {
                        dependents.remove(pos);
                    }
                    if dependents.is_empty() {
                        self.dependents_graph.remove(&dep);
                    }
                }
            }
        }

        if deps.is_empty() {
            return;
        }

        self.dependency_graph.insert(job_id, deps.clone());
        for dep in deps {
            let entry = self.dependents_graph.entry(dep).or_default();
            match entry.binary_search(&job_id) {
                Ok(_) => {}
                Err(pos) => entry.insert(pos, job_id),
            }
        }
    }

    fn are_dependencies_satisfied_now(&self, spec: &JobSpec) -> bool {
        if spec.depends_on.is_none() && spec.depends_on_ids.is_empty() {
            return true;
        }

        let mut dep_ids: Vec<u32> = spec.depends_on_ids.iter().copied().collect();
        if let Some(dep) = spec.depends_on {
            if !dep_ids.contains(&dep) {
                dep_ids.push(dep);
            }
        }

        match spec
            .dependency_mode
            .as_ref()
            .unwrap_or(&DependencyMode::All)
        {
            DependencyMode::All => dep_ids.iter().all(|dep_id| {
                self.get_job_runtime(*dep_id)
                    .is_some_and(|rt| rt.state == JobState::Finished)
            }),
            DependencyMode::Any => dep_ids.iter().any(|dep_id| {
                self.get_job_runtime(*dep_id)
                    .is_some_and(|rt| rt.state == JobState::Finished)
            }),
        }
    }

    fn queued_dependency_reason_for_job(&self, job_id: u32) -> Option<JobStateReason> {
        let (spec, rt) = self.get_job_parts(job_id)?;
        if rt.state != JobState::Queued {
            return None;
        }

        if self.are_dependencies_satisfied_now(spec) {
            None
        } else {
            Some(JobStateReason::WaitingForDependency)
        }
    }

    pub(crate) fn sync_queued_dependency_reason(&mut self, job_id: u32) {
        let reason = self.queued_dependency_reason_for_job(job_id);
        if let Some(rt) = self.get_job_runtime_mut(job_id) {
            rt.reason = reason.map(Box::new);
        }
    }

    fn refresh_dependency_reasons_for_dependents(&mut self, dependency_job_id: u32) {
        let dependent_job_ids = self
            .dependents_graph
            .get(&dependency_job_id)
            .cloned()
            .unwrap_or_default();

        for job_id in dependent_job_ids {
            if self
                .get_job_runtime(job_id)
                .is_none_or(|rt| rt.state != JobState::Queued)
            {
                continue;
            }

            let desired_reason = self.queued_dependency_reason_for_job(job_id);
            let current_reason = self
                .get_job_runtime(job_id)
                .and_then(|rt| rt.reason.as_deref().cloned());

            let should_update = match desired_reason {
                Some(JobStateReason::WaitingForDependency) => true,
                None => {
                    current_reason.is_none()
                        || matches!(current_reason, Some(JobStateReason::WaitingForDependency))
                }
                Some(_) => false,
            };

            if should_update {
                if let Some(rt) = self.get_job_runtime_mut(job_id) {
                    rt.reason = desired_reason.map(Box::new);
                }
            }
        }
    }

    pub fn submit_job(&mut self, job: Job) -> (u32, String) {
        let job_id = self.next_job_id;
        self.next_job_id += 1;

        let submitted_at = std::time::SystemTime::now();

        // Split incoming legacy `Job` and normalize runtime-managed fields.
        let (mut spec, mut runtime) = job.into_parts();

        let run_name = spec
            .run_name
            .take()
            .unwrap_or_else(|| format_compact!("gjob-{}", job_id));

        // Persisted/spec fields
        spec.run_name = Some(run_name.clone());
        spec.submitted_at = Some(submitted_at);

        // Hot/runtime fields
        runtime.id = job_id;
        runtime.state = JobState::Queued;
        runtime.gpu_ids = None;
        runtime.started_at = None;
        runtime.finished_at = None;
        runtime.reason = None;

        // Update user jobs index (used by dependency shorthand resolution).
        self.user_jobs_index
            .entry(spec.submitted_by.clone())
            .or_default()
            .push(job_id);

        // Update state index.
        self.state_jobs_index
            .entry(runtime.state)
            .or_default()
            .push(job_id);

        // Update project index (maintains sorted order).
        self.update_project_jobs_index(job_id, None, spec.project.as_ref());

        // Update dependency graph only if job has dependencies.
        let mut deps: Vec<u32> = spec.depends_on_ids.iter().copied().collect();
        if let Some(dep) = spec.depends_on {
            if !deps.contains(&dep) {
                deps.push(dep);
            }
        }

        // Store split representation only (no large Vec<Job> in memory).
        self.job_specs.push(spec);
        self.job_runtimes.push(runtime);
        self.replace_job_dependencies_index(job_id, deps);
        self.sync_queued_dependency_reason(job_id);
        self.check_invariant();

        (job_id, run_name.into())
    }

    /// Update the cached dependency graph entry for a job.
    ///
    /// This affects:
    /// - circular dependency validation (`validate_no_circular_dependency`)
    ///
    /// Scheduling itself uses `JobSpec` directly, so this cache is only for validation speed.
    pub fn set_job_dependencies(&mut self, job_id: u32, deps: Vec<u32>) {
        self.replace_job_dependencies_index(job_id, deps);
    }

    /// Update group_running_count index when a job transitions states
    /// This maintains O(1) lookup for group concurrency checks
    pub(super) fn update_group_running_count(
        &mut self,
        group_id: Option<uuid::Uuid>,
        old_state: JobState,
        new_state: JobState,
    ) {
        // Only update if transitioning to/from Running state
        let entering_running = new_state == JobState::Running && old_state != JobState::Running;
        let leaving_running = old_state == JobState::Running && new_state != JobState::Running;

        if !entering_running && !leaving_running {
            return;
        }

        if let Some(group_id) = group_id {
            if entering_running {
                *self.group_running_count.entry(group_id).or_insert(0) += 1;
            } else if leaving_running {
                if let Some(count) = self.group_running_count.get_mut(&group_id) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.group_running_count.remove(&group_id);
                    }
                }
            }
        }
    }

    /// Unified state transition with automatic index updates.
    ///
    /// This is the "single choke point" that should be used for any transition that may
    /// affect indices (e.g. `group_running_count`).
    pub(super) fn transition_job_state(
        &mut self,
        job_id: u32,
        next: JobState,
        reason: Option<JobStateReason>,
    ) -> Option<bool> {
        let (group_id, old_state, transitioned) = (|| {
            let rt = self.get_job_runtime_mut(job_id)?;
            let group_id = rt.group_id;
            let old_state = rt.state;

            if old_state == next {
                tracing::warn!(
                    "Job {} already in state {}, ignoring transition",
                    job_id,
                    next
                );
                return Some((group_id, old_state, false));
            }

            if !old_state.can_transition_to(next) {
                tracing::error!(
                    "Job {} invalid transition: {} → {}",
                    job_id,
                    old_state,
                    next
                );
                return Some((group_id, old_state, false));
            }

            // Keep timestamp mutation consistent with Job's transition logic.
            match next {
                JobState::Running => rt.started_at = Some(std::time::SystemTime::now()),
                JobState::Finished | JobState::Failed | JobState::Cancelled | JobState::Timeout => {
                    rt.finished_at = Some(std::time::SystemTime::now())
                }
                _ => {}
            }

            rt.reason = reason.map(Box::new);

            rt.state = next;
            tracing::debug!("Job {} transitioned to {}", job_id, next);
            Some((group_id, old_state, true))
        })()?;

        if transitioned {
            self.update_group_running_count(group_id, old_state, next);
            self.update_state_jobs_index(job_id, old_state, next);
        }

        Some(transitioned)
    }

    /// Finish a job and return whether auto_close_tmux is enabled along with run_name
    /// Returns: Some((should_close_tmux, run_name)) if job exists, None otherwise
    /// Note: Caller is responsible for persisting state and closing tmux if needed
    pub fn finish_job(&mut self, job_id: u32) -> Option<(bool, Option<String>)> {
        let spec = self.get_job_spec(job_id)?;
        let should_close_tmux = spec.auto_close_tmux;
        let run_name = spec.run_name.as_ref().map(|s| s.to_string());

        // Attempt transition, but preserve the historical behavior of returning `Some(...)`
        // as long as the job exists.
        self.transition_job_state(job_id, JobState::Finished, None)?;
        self.refresh_dependency_reasons_for_dependents(job_id);

        Some((should_close_tmux, run_name))
    }

    pub fn fail_job(&mut self, job_id: u32) -> bool {
        let result = self
            .transition_job_state(job_id, JobState::Failed, None)
            .is_some();
        if result {
            self.refresh_dependency_reasons_for_dependents(job_id);
        }
        result
    }

    pub fn timeout_job(&mut self, job_id: u32) -> bool {
        let result = self
            .transition_job_state(job_id, JobState::Timeout, None)
            .is_some();
        if result {
            self.refresh_dependency_reasons_for_dependents(job_id);
        }
        result
    }

    /// Cancel a job and return run_name if it needs Ctrl-C (was Running)
    /// Note: Caller is responsible for sending Ctrl-C and persisting state
    pub fn cancel_job(
        &mut self,
        job_id: u32,
        reason: Option<JobStateReason>,
    ) -> Option<(bool, Option<String>)> {
        let was_running = self.get_job_runtime(job_id)?.state == JobState::Running;
        let run_name = self
            .get_job_spec(job_id)?
            .run_name
            .as_ref()
            .map(|s| s.to_string());

        let reason = reason.unwrap_or(JobStateReason::CancelledByUser);
        self.transition_job_state(job_id, JobState::Cancelled, Some(reason))?;
        self.refresh_dependency_reasons_for_dependents(job_id);

        Some((was_running, run_name))
    }

    pub fn hold_job(&mut self, job_id: u32) -> bool {
        self.transition_job_state(job_id, JobState::Hold, None)
            .is_some()
    }

    pub fn release_job(&mut self, job_id: u32) -> bool {
        let result = self
            .transition_job_state(job_id, JobState::Queued, None)
            .is_some();
        if result {
            self.sync_queued_dependency_reason(job_id);
        }
        result
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

    /// Check if job's dependencies are satisfied (using split spec/runtime)
    pub(super) fn are_dependencies_satisfied_split(
        spec: &JobSpec,
        finished_jobs: &std::collections::HashSet<u32>,
    ) -> bool {
        // Check if job has no dependencies
        if spec.depends_on.is_none() && spec.depends_on_ids.is_empty() {
            return true;
        }

        // Collect all dependency IDs
        let mut dep_ids: Vec<u32> = spec.depends_on_ids.iter().copied().collect();
        if let Some(dep) = spec.depends_on {
            if !dep_ids.contains(&dep) {
                dep_ids.push(dep);
            }
        }

        match spec
            .dependency_mode
            .as_ref()
            .unwrap_or(&DependencyMode::All)
        {
            DependencyMode::All => dep_ids.iter().all(|dep_id| finished_jobs.contains(dep_id)),
            DependencyMode::Any => dep_ids.iter().any(|dep_id| finished_jobs.contains(dep_id)),
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
                .dependents_graph
                .get(&current_failed_id)
                .into_iter()
                .flatten()
                .copied()
                .filter(|job_id| {
                    self.get_job_parts(*job_id).is_some_and(|(spec, rt)| {
                        rt.state == JobState::Queued && spec.auto_cancel_on_dependency_failure
                    })
                })
                .collect();

            for job_id in dependent_job_ids {
                let transitioned = self
                    .transition_job_state(
                        job_id,
                        JobState::Cancelled,
                        Some(JobStateReason::DependencyFailed(current_failed_id)),
                    )
                    .unwrap_or(false);
                if !transitioned {
                    continue;
                }

                tracing::info!(
                    "Auto-cancelled job {} due to failed dependency {}",
                    job_id,
                    current_failed_id
                );
                all_cancelled_jobs.push(job_id);
                // Add this cancelled job to the queue to check its dependents.
                jobs_to_process.push(job_id);
            }
        }

        all_cancelled_jobs
    }

    /// Validate that a job can be updated
    /// Returns Ok(()) if update is valid, Err(String) with error message otherwise
    pub fn validate_job_update(&self, job_id: u32, new_deps: Option<&[u32]>) -> Result<(), String> {
        let rt = self
            .get_job_runtime(job_id)
            .ok_or_else(|| format!("Job {} not found", job_id))?;

        // Check if job is in updatable state (Queued or Hold)
        if rt.state != JobState::Queued && rt.state != JobState::Hold {
            return Err(format!(
                "Job {} is in state '{}' and cannot be updated. Only queued or held jobs can be updated.",
                job_id, rt.state
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
}
