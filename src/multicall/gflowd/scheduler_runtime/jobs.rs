use super::*;

impl SchedulerRuntime {
    /// Re-adopt persisted Running jobs before scheduling starts. A live runner
    /// remains Running; a durable result is finalized immediately; a missing
    /// runner/result is treated as an execution failure so resources cannot be
    /// stranded forever after daemon downtime.
    pub(crate) async fn recover_running_jobs(&mut self) -> Vec<RecoveryOutcome> {
        let running_jobs: Vec<Job> = self
            .jobs()
            .into_iter()
            .filter(|job| job.state == JobState::Running)
            .collect();
        let mut outcomes = Vec::new();

        for job in running_jobs {
            let status = self
                .executor
                .execution_status(job.id, job.run_name.as_deref());
            match status {
                ExecutionStatus::Running => {
                    tracing::info!(job_id = job.id, "Re-adopted running job runner");
                }
                ExecutionStatus::Finished(result) => {
                    tracing::info!(
                        job_id = job.id,
                        exit_code = ?result.exit_code,
                        signal = ?result.signal,
                        "Recovered completed job runner result"
                    );
                    if let Some(outcome) = self.finalize_recovered_job(job, result).await {
                        outcomes.push(outcome);
                    }
                }
                ExecutionStatus::Missing => {
                    tracing::warn!(
                        job_id = job.id,
                        "Running job has no live runner or durable result; marking it failed"
                    );
                    let job_id = job.id;
                    if let Some(outcome) = self
                        .finalize_recovered_job(
                            job,
                            ExecutionResult {
                                job_id,
                                exit_code: None,
                                signal: None,
                            },
                        )
                        .await
                    {
                        outcomes.push(outcome);
                    }
                }
            }
        }

        outcomes
    }

    async fn finalize_recovered_job(
        &mut self,
        job: Job,
        result: ExecutionResult,
    ) -> Option<RecoveryOutcome> {
        let gpu_ids = job.gpu_ids.clone();
        let memory_mb = job.memory_limit_mb;

        if result.succeeded() {
            if self.finish_job(job.id).await {
                return Some(RecoveryOutcome {
                    job_id: job.id,
                    final_state: JobState::Finished,
                    gpu_ids,
                    memory_mb,
                    retry_job_id: None,
                });
            }
            return None;
        }

        let retry_job_id = self.fail_job(job.id).await?;
        Some(RecoveryOutcome {
            job_id: job.id,
            final_state: JobState::Failed,
            gpu_ids,
            memory_mb,
            retry_job_id,
        })
    }

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

    /// Enforce queue-depth quotas (`max_queued_jobs`) at submission time.
    /// Running/GPU quotas are enforced by the scheduling loop instead, where
    /// exceeding them means waiting in the queue rather than rejection.
    fn check_queue_quota(
        &self,
        job: &Job,
        user_pending_bias: &HashMap<CompactString, usize>,
        project_pending_bias: &HashMap<CompactString, usize>,
    ) -> Result<()> {
        if let Some(reason) = self.scheduler.check_queue_quota(
            &job.submitted_by,
            job.project.as_deref(),
            user_pending_bias,
            project_pending_bias,
        ) {
            bail!("{reason}");
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
        self.check_queue_quota(&job, &HashMap::new(), &HashMap::new())?;
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
        let batch_size = jobs.len();
        if batch_size > 1000 {
            bail!("Batch size exceeds maximum of 1000 jobs");
        }

        let mut reserved_names = self.current_reserved_run_names();
        let mut normalized_jobs = Vec::with_capacity(batch_size);
        // Queue-depth quotas must account for jobs accepted earlier in this
        // same batch, which are not indexed until `scheduler.submit_job` runs.
        let mut user_pending_bias: HashMap<CompactString, usize> = HashMap::new();
        let mut project_pending_bias: HashMap<CompactString, usize> = HashMap::new();
        for (next_job_id, mut job) in (self.scheduler.next_job_id()..).zip(jobs) {
            self.normalize_and_validate_project(&mut job)?;
            Self::validate_shared_job_requirements(&job)?;
            self.check_queue_quota(&job, &user_pending_bias, &project_pending_bias)?;
            *user_pending_bias
                .entry(job.submitted_by.clone())
                .or_default() += 1;
            if let Some(ref project) = job.project {
                *project_pending_bias.entry(project.clone()).or_default() += 1;
            }
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
        let finished = self.scheduler.finish_job(job_id).is_some();
        if finished {
            self.mark_dirty();
            // Fetch the job *after* the transition so the executor sees the
            // terminal state (tmux: auto-close the session on Finished).
            if let Some(job) = self.scheduler.get_job(job_id) {
                self.executor.cleanup(&job);
            }
        }
        finished
    }

    pub async fn fail_job(&mut self, job_id: u32) -> Option<Option<u32>> {
        let result = self.finalize_job_with_retry(job_id, JobState::Failed).await;
        if result.is_some() {
            if let Some(job) = self.scheduler.get_job(job_id) {
                self.executor.cleanup(&job);
            }
        }
        result
    }

    pub async fn explicit_fail_job(&mut self, job_id: u32) -> bool {
        let result = self.scheduler.fail_job(job_id);
        if result {
            self.mark_dirty();
            if let Some(job) = self.scheduler.get_job(job_id) {
                self.executor.cleanup(&job);
            }
        }
        result
    }

    pub async fn timeout_job(&mut self, job_id: u32) -> Option<Option<u32>> {
        let result = self
            .finalize_job_with_retry(job_id, JobState::Timeout)
            .await;
        if result.is_some() {
            if let Some(job) = self.scheduler.get_job(job_id) {
                self.executor.cleanup(&job);
            }
        }
        result
    }

    pub async fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some((was_running, _run_name)) = self.scheduler.cancel_job(job_id, None) {
            self.mark_dirty();

            // If the job was running, terminate its execution (process executor:
            // SIGTERM the process group; tmux: send Ctrl-C), then clean up.
            if was_running {
                let job = self.scheduler.get_job(job_id);
                if let Err(e) = self
                    .executor
                    .terminate(job_id, job.as_ref().and_then(|j| j.run_name.as_deref()))
                {
                    tracing::error!(job_id, error = %e, "Failed to terminate job on cancel");
                }

                // Wait a moment for graceful shutdown, then run executor
                // cleanup (tmux: stop pipe-pane; process: no-op).
                tokio::time::sleep(Duration::from_millis(500)).await;
                if let Some(job) = job {
                    self.executor.cleanup(&job);
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
        request: crate::multicall::gflowd::server::UpdateJobRequest,
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
}
