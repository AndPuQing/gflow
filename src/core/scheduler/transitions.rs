use super::*;
use std::collections::VecDeque;

impl Scheduler {
    pub(super) fn normalized_dependency_ids(spec: &JobSpec) -> Vec<u32> {
        let mut deps: Vec<u32> = spec.depends_on_ids.iter().copied().collect();
        if let Some(dep) = spec.depends_on {
            if !deps.contains(&dep) {
                deps.push(dep);
            }
        }
        deps
    }

    pub(crate) fn dependency_ids_for_job(&self, job_id: u32) -> Vec<u32> {
        self.get_job_spec(job_id)
            .map(Self::normalized_dependency_ids)
            .unwrap_or_default()
    }

    fn dependency_mode(spec: &JobSpec) -> DependencyMode {
        spec.dependency_mode.unwrap_or(DependencyMode::All)
    }

    pub(super) fn build_dependency_runtime(&self, job_id: u32) -> DependencyRuntime {
        let Some(spec) = self.get_job_spec(job_id) else {
            return DependencyRuntime::default();
        };

        let deps = Self::normalized_dependency_ids(spec);
        let total = deps.len() as u32;
        let mut success = 0;
        let mut terminal_non_success = 0;

        for dep_id in deps {
            let Some(rt) = self.get_job_runtime(dep_id) else {
                continue;
            };
            match rt.state.dependency_outcome() {
                Some(true) => success += 1,
                Some(false) => terminal_non_success += 1,
                _ => {}
            }
        }

        let mode = Self::dependency_mode(spec);
        let deps_satisfied = if total == 0 {
            true
        } else {
            match mode {
                DependencyMode::All => success == total,
                DependencyMode::Any => success > 0,
            }
        };
        let impossible = if total == 0 {
            false
        } else {
            match mode {
                DependencyMode::All => terminal_non_success > 0,
                DependencyMode::Any => success == 0 && success + terminal_non_success == total,
            }
        };

        DependencyRuntime {
            total,
            success,
            terminal_non_success,
            deps_satisfied,
            impossible,
            ..DependencyRuntime::default()
        }
    }

    pub(super) fn dependency_runtime(&self, job_id: u32) -> Option<&DependencyRuntime> {
        let idx = job_id.checked_sub(1)? as usize;
        self.dependency_runtimes.get(idx)
    }

    fn dependency_runtime_mut(&mut self, job_id: u32) -> Option<&mut DependencyRuntime> {
        let idx = job_id.checked_sub(1)? as usize;
        self.dependency_runtimes.get_mut(idx)
    }

    pub(super) fn set_dependency_runtime(&mut self, job_id: u32, next: DependencyRuntime) {
        let idx = (job_id - 1) as usize;
        if let Some(slot) = self.dependency_runtimes.get_mut(idx) {
            let ready_epoch = slot.ready_epoch;
            *slot = next;
            slot.ready_epoch = ready_epoch;
        }
    }

    fn add_dependent_edge(&mut self, dep_id: u32, job_id: u32) {
        let entry = self.dependents_graph.entry(dep_id).or_default();
        match entry.binary_search(&job_id) {
            Ok(_) => {}
            Err(pos) => entry.insert(pos, job_id),
        }
    }

    fn remove_dependent_edge(&mut self, dep_id: u32, job_id: u32) {
        if let Some(dependents) = self.dependents_graph.get_mut(&dep_id) {
            if let Ok(pos) = dependents.binary_search(&job_id) {
                dependents.remove(pos);
            }
            if dependents.is_empty() {
                self.dependents_graph.remove(&dep_id);
            }
        }
    }

    pub(super) fn insert_job_dependencies_index(&mut self, job_id: u32, deps: &[u32]) {
        for &dep_id in deps {
            self.add_dependent_edge(dep_id, job_id);
        }
    }

    fn replace_job_dependencies_index(&mut self, job_id: u32, old_deps: &[u32], new_deps: &[u32]) {
        for &dep_id in old_deps {
            self.remove_dependent_edge(dep_id, job_id);
        }
        self.insert_job_dependencies_index(job_id, new_deps);
    }

    fn bump_ready_epoch(&mut self, job_id: u32) {
        if let Some(dep_rt) = self.dependency_runtime_mut(job_id) {
            dep_rt.ready_epoch = dep_rt.ready_epoch.wrapping_add(1);
        }
    }

    pub(super) fn enqueue_if_ready(&mut self, job_id: u32) {
        let Some(rt) = self.get_job_runtime(job_id) else {
            return;
        };
        if rt.state != JobState::Queued {
            return;
        }

        let Some(dep_rt) = self.dependency_runtime(job_id) else {
            return;
        };
        if !dep_rt.deps_satisfied {
            return;
        }

        self.ready_heap.push(ReadyEntry {
            job_id,
            epoch: dep_rt.ready_epoch,
            priority: rt.priority,
            time_bonus: Self::calculate_time_bonus(&rt.time_limit),
        });
    }

    fn queued_dependency_reason_for_job(&self, job_id: u32) -> Option<JobStateReason> {
        let (_spec, rt) = self.get_job_parts(job_id)?;
        if rt.state != JobState::Queued {
            return None;
        }

        if self
            .dependency_runtime(job_id)
            .is_some_and(|dep_rt| dep_rt.deps_satisfied)
        {
            None
        } else {
            Some(JobStateReason::WaitingForDependency)
        }
    }

    pub(crate) fn sync_queued_dependency_reason(&mut self, job_id: u32) {
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

    fn dependency_failure_cause(&self, job_id: u32) -> Option<u32> {
        let spec = self.get_job_spec(job_id)?;
        let deps = Self::normalized_dependency_ids(spec);
        let mode = Self::dependency_mode(spec);

        let has_success = deps.iter().copied().any(|dep_id| {
            self.get_job_runtime(dep_id)
                .is_some_and(|rt| rt.state.dependency_outcome() == Some(true))
        });

        deps.into_iter().find(|&dep_id| {
            self.get_job_runtime(dep_id).is_some_and(|rt| match mode {
                DependencyMode::All => rt.state.dependency_outcome() == Some(false),
                DependencyMode::Any => !has_success && rt.state.dependency_outcome() == Some(false),
            })
        })
    }

    fn refresh_single_job_readiness(&mut self, job_id: u32) -> bool {
        if !self.job_exists(job_id) {
            return false;
        }

        let old_reason = self.queued_dependency_reason_for_job(job_id);

        self.bump_ready_epoch(job_id);
        let dep_rt = self.build_dependency_runtime(job_id);
        self.set_dependency_runtime(job_id, dep_rt);
        self.sync_queued_dependency_reason(job_id);

        let should_auto_cancel = self.get_job_parts(job_id).is_some_and(|(spec, rt)| {
            rt.state == JobState::Queued
                && spec.auto_cancel_on_dependency_failure
                && self
                    .dependency_runtime(job_id)
                    .is_some_and(|dep_rt| dep_rt.impossible)
        });

        if should_auto_cancel {
            let failed_dep = self.dependency_failure_cause(job_id).unwrap_or(job_id);
            let _ = self.transition_job_state(
                job_id,
                JobState::Cancelled,
                Some(JobStateReason::DependencyFailed(failed_dep)),
            );
            return true;
        }

        if self
            .dependency_runtime(job_id)
            .is_some_and(|dep_rt| dep_rt.deps_satisfied)
        {
            self.enqueue_if_ready(job_id);
        }

        self.queued_dependency_reason_for_job(job_id) != old_reason
    }

    fn refresh_queued_dependency_reason_wavefront(&mut self, source_job_id: u32) {
        let mut queue = VecDeque::from([source_job_id]);
        let mut seen = HashSet::new();

        while let Some(current_job_id) = queue.pop_front() {
            if !seen.insert(current_job_id) {
                continue;
            }

            let dependent_job_ids = self
                .dependents_graph
                .get(&current_job_id)
                .cloned()
                .unwrap_or_default();

            for job_id in dependent_job_ids {
                let is_queued = self
                    .get_job_runtime(job_id)
                    .is_some_and(|rt| rt.state == JobState::Queued);
                if !is_queued {
                    continue;
                }

                if self.refresh_single_job_readiness(job_id) {
                    queue.push_back(job_id);
                }
            }
        }
    }

    pub(crate) fn refresh_job_readiness(&mut self, job_id: u32) {
        if !self.job_exists(job_id) {
            return;
        }

        let reason_changed = self.refresh_single_job_readiness(job_id);
        if reason_changed {
            self.refresh_queued_dependency_reason_wavefront(job_id);
        }
    }

    fn propagate_terminal_state_to_dependents(
        &mut self,
        source_job_id: u32,
        final_state: JobState,
    ) {
        let Some(success) = final_state.dependency_outcome() else {
            return;
        };

        let mut sources_to_process = vec![(source_job_id, success)];
        while let Some((current_source_id, current_success)) = sources_to_process.pop() {
            let dependent_job_ids = self
                .dependents_graph
                .get(&current_source_id)
                .cloned()
                .unwrap_or_default();

            for job_id in dependent_job_ids {
                let Some(spec) = self.get_job_spec(job_id) else {
                    continue;
                };
                let mode = Self::dependency_mode(spec);
                let auto_cancel = spec.auto_cancel_on_dependency_failure;

                let (became_ready, became_impossible) = {
                    let Some(dep_rt) = self.dependency_runtime_mut(job_id) else {
                        continue;
                    };
                    let was_ready = dep_rt.deps_satisfied;
                    let was_impossible = dep_rt.impossible;

                    if current_success {
                        dep_rt.success += 1;
                    } else {
                        dep_rt.terminal_non_success += 1;
                    }

                    dep_rt.deps_satisfied = if dep_rt.total == 0 {
                        true
                    } else {
                        match mode {
                            DependencyMode::All => dep_rt.success == dep_rt.total,
                            DependencyMode::Any => dep_rt.success > 0,
                        }
                    };
                    dep_rt.impossible = if dep_rt.total == 0 {
                        false
                    } else {
                        match mode {
                            DependencyMode::All => dep_rt.terminal_non_success > 0,
                            DependencyMode::Any => {
                                dep_rt.success == 0
                                    && dep_rt.success + dep_rt.terminal_non_success == dep_rt.total
                            }
                        }
                    };

                    (
                        !was_ready && dep_rt.deps_satisfied,
                        !was_impossible && dep_rt.impossible,
                    )
                };

                self.sync_queued_dependency_reason(job_id);

                let should_auto_cancel = self.get_job_runtime(job_id).is_some_and(|rt| {
                    rt.state == JobState::Queued && auto_cancel && became_impossible
                });
                if should_auto_cancel {
                    let transitioned = self
                        .transition_job_state(
                            job_id,
                            JobState::Cancelled,
                            Some(JobStateReason::DependencyFailed(current_source_id)),
                        )
                        .unwrap_or(false);
                    if transitioned {
                        tracing::info!(
                            "Auto-cancelled job {} due to failed dependency {}",
                            job_id,
                            current_source_id
                        );
                        sources_to_process.push((job_id, false));
                    }
                    continue;
                }

                if became_ready {
                    self.enqueue_if_ready(job_id);
                }
            }
        }
    }

    pub fn submit_job(&mut self, job: Job) -> (u32, String) {
        let job_id = self.next_job_id;
        self.next_job_id += 1;

        let submitted_at = std::time::SystemTime::now();

        let (mut spec, mut runtime) = job.into_parts();
        let deps = Self::normalized_dependency_ids(&spec);

        let run_name = spec
            .run_name
            .take()
            .unwrap_or_else(|| format_compact!("gjob-{}", job_id));

        spec.run_name = Some(run_name.clone());
        spec.submitted_at = Some(submitted_at);

        runtime.id = job_id;
        runtime.state = JobState::Queued;
        runtime.gpu_ids = None;
        runtime.started_at = None;
        runtime.finished_at = None;
        runtime.reason = None;

        self.user_jobs_index
            .entry(spec.submitted_by.clone())
            .or_default()
            .push(job_id);
        self.state_jobs_index
            .entry(runtime.state)
            .or_default()
            .push(job_id);
        self.update_project_jobs_index(job_id, None, spec.project.as_ref());

        self.job_specs.push(spec);
        self.job_runtimes.push(runtime);
        self.dependency_runtimes.push(DependencyRuntime::default());

        self.insert_job_dependencies_index(job_id, &deps);
        self.refresh_job_readiness(job_id);
        self.check_invariant();

        (job_id, run_name.into())
    }

    pub fn replace_job_dependencies(
        &mut self,
        job_id: u32,
        old_deps: Vec<u32>,
        new_deps: Vec<u32>,
    ) {
        self.replace_job_dependencies_index(job_id, &old_deps, &new_deps);
        self.refresh_job_readiness(job_id);
    }

    pub(super) fn update_group_running_count(
        &mut self,
        group_id: Option<uuid::Uuid>,
        old_state: JobState,
        new_state: JobState,
    ) {
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
            self.bump_ready_epoch(job_id);

            match next {
                JobState::Queued => self.refresh_job_readiness(job_id),
                JobState::Finished | JobState::Failed | JobState::Cancelled | JobState::Timeout => {
                    self.propagate_terminal_state_to_dependents(job_id, next);
                }
                JobState::Hold | JobState::Running => {}
            }
        }

        Some(transitioned)
    }

    pub fn finish_job(&mut self, job_id: u32) -> Option<(bool, Option<String>)> {
        let spec = self.get_job_spec(job_id)?;
        let should_close_tmux = spec.auto_close_tmux;
        let run_name = spec.run_name.as_ref().map(|s| s.to_string());

        self.transition_job_state(job_id, JobState::Finished, None)?;

        Some((should_close_tmux, run_name))
    }

    pub fn retry_job_after_failure(&mut self, job_id: u32) -> Option<u32> {
        let (group_id, old_state, required_memory, next_attempt) = {
            let (spec, runtime) = self.get_job_parts_mut(job_id)?;

            if runtime.state != JobState::Running {
                return None;
            }

            let max_retry = spec.max_retry.unwrap_or(0);
            if runtime.retry_attempt >= max_retry {
                return None;
            }

            let group_id = runtime.group_id;
            let old_state = runtime.state;
            let required_memory = runtime.memory_limit_mb.unwrap_or(0);

            runtime.retry_attempt = runtime.retry_attempt.saturating_add(1);
            runtime.state = JobState::Queued;
            runtime.gpu_ids = None;
            runtime.started_at = None;
            runtime.finished_at = None;
            runtime.reason = None;

            (group_id, old_state, required_memory, runtime.retry_attempt)
        };

        self.available_memory_mb = self.available_memory_mb.saturating_add(required_memory);
        self.update_group_running_count(group_id, old_state, JobState::Queued);
        self.update_state_jobs_index(job_id, old_state, JobState::Queued);
        self.bump_ready_epoch(job_id);
        self.refresh_job_readiness(job_id);

        Some(next_attempt)
    }

    pub fn fail_job(&mut self, job_id: u32) -> bool {
        self.transition_job_state(job_id, JobState::Failed, None)
            .is_some()
    }

    pub fn timeout_job(&mut self, job_id: u32) -> bool {
        self.transition_job_state(job_id, JobState::Timeout, None)
            .is_some()
    }

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

        Some((was_running, run_name))
    }

    pub fn hold_job(&mut self, job_id: u32) -> bool {
        self.transition_job_state(job_id, JobState::Hold, None)
            .is_some()
    }

    pub fn release_job(&mut self, job_id: u32) -> bool {
        self.transition_job_state(job_id, JobState::Queued, None)
            .is_some()
    }

    pub fn resolve_dependency(&self, username: &str, shorthand: &str) -> Option<u32> {
        let trimmed = shorthand.trim();

        if trimmed.is_empty() {
            return None;
        }

        let user_jobs = self.user_jobs_index.get(username)?;

        if trimmed == "@" {
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

    pub fn validate_no_circular_dependency(
        &self,
        new_job_id: u32,
        dependency_ids: &[u32],
    ) -> Result<(), String> {
        use std::collections::HashSet;

        for &dep_id in dependency_ids {
            if self.has_path_dfs(dep_id, new_job_id, &mut HashSet::new()) {
                return Err(format!(
                    "Circular dependency detected: Job {} depends on Job {}, \
                     which has a path back to Job {}",
                    new_job_id, dep_id, new_job_id
                ));
            }
        }

        Ok(())
    }

    fn has_path_dfs(
        &self,
        current: u32,
        target: u32,
        visited: &mut std::collections::HashSet<u32>,
    ) -> bool {
        if current == target {
            return true;
        }

        if !visited.insert(current) {
            return false;
        }

        let neighbors = self
            .get_job_spec(current)
            .map(Self::normalized_dependency_ids)
            .unwrap_or_default();

        for neighbor in neighbors {
            if self.has_path_dfs(neighbor, target, visited) {
                return true;
            }
        }

        false
    }

    pub fn auto_cancel_dependent_jobs(&mut self, failed_job_id: u32) -> Vec<u32> {
        let mut cancelled = Vec::new();
        let mut sources_to_process = vec![failed_job_id];
        let mut seen = HashSet::new();

        while let Some(source_id) = sources_to_process.pop() {
            let dependent_job_ids = self
                .dependents_graph
                .get(&source_id)
                .cloned()
                .unwrap_or_default();

            for job_id in dependent_job_ids {
                if !seen.insert((source_id, job_id)) {
                    continue;
                }

                let should_cancel = self.get_job_parts(job_id).is_some_and(|(spec, rt)| {
                    rt.state == JobState::Queued
                        && spec.auto_cancel_on_dependency_failure
                        && self
                            .dependency_runtime(job_id)
                            .is_some_and(|dep_rt| dep_rt.impossible)
                });
                if !should_cancel {
                    continue;
                }

                let transitioned = self
                    .transition_job_state(
                        job_id,
                        JobState::Cancelled,
                        Some(JobStateReason::DependencyFailed(source_id)),
                    )
                    .unwrap_or(false);
                if transitioned {
                    cancelled.push(job_id);
                    sources_to_process.push(job_id);
                }
            }
        }

        cancelled
    }

    pub fn validate_job_update(&self, job_id: u32, new_deps: Option<&[u32]>) -> Result<(), String> {
        let rt = self
            .get_job_runtime(job_id)
            .ok_or_else(|| format!("Job {} not found", job_id))?;

        if rt.state != JobState::Queued && rt.state != JobState::Hold {
            return Err(format!(
                "Job {} is in state '{}' and cannot be updated. Only queued or held jobs can be updated.",
                job_id, rt.state
            ));
        }

        if let Some(deps) = new_deps {
            for &dep_id in deps {
                if !self.job_exists(dep_id) {
                    return Err(format!("Dependency job {} does not exist", dep_id));
                }
            }

            self.validate_no_circular_dependency(job_id, deps)?;
        }

        Ok(())
    }
}
