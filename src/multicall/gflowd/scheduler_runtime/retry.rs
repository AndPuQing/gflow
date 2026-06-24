use super::*;

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

    pub(super) async fn finalize_job_with_retry(
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
}
