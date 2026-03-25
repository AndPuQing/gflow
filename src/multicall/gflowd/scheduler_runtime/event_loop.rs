use super::super::events::{EventBus, EventEnvelope, SchedulerEvent};
use super::*;
use crate::core::scheduler::ExecutionFailureOutcome;
use std::sync::Arc;
use tracing::Instrument;

/// Event-driven scheduling loop
pub async fn run_event_driven(
    shared_state: SharedState,
    event_bus: Arc<EventBus>,
    gpu_poll_interval: Duration,
) {
    // Spawn all event handlers and monitors
    let handles = vec![
        // Scheduler trigger handler with debouncing
        tokio::spawn(
            scheduler_trigger_handler_with_debounce(
                event_bus.subscribe(),
                Arc::clone(&shared_state),
                Arc::clone(&event_bus),
            )
            .instrument(tracing::info_span!("scheduler_trigger_task")),
        ),
        // GPU monitor - polls NVML using the configured interval
        tokio::spawn(
            super::monitors::gpu_monitor_task(
                Arc::clone(&shared_state),
                Arc::clone(&event_bus),
                gpu_poll_interval,
            )
            .instrument(tracing::info_span!("gpu_monitor_task")),
        ),
        // Zombie monitor - checks tmux every 30s
        tokio::spawn(
            super::monitors::zombie_monitor_task(Arc::clone(&shared_state), Arc::clone(&event_bus))
                .instrument(tracing::info_span!("zombie_monitor_task")),
        ),
        // Zombie handler - reacts to zombie events
        tokio::spawn(
            super::monitors::zombie_handler_task(event_bus.subscribe(), Arc::clone(&shared_state))
                .instrument(tracing::info_span!("zombie_handler_task")),
        ),
        // Timeout monitor - checks time limits every 10s
        tokio::spawn(
            super::monitors::timeout_monitor_task(
                Arc::clone(&shared_state),
                Arc::clone(&event_bus),
            )
            .instrument(tracing::info_span!("timeout_monitor_task")),
        ),
        // Timeout handler - reacts to timeout events
        tokio::spawn(
            super::monitors::timeout_handler_task(event_bus.subscribe(), Arc::clone(&shared_state))
                .instrument(tracing::info_span!("timeout_handler_task")),
        ),
        // Reservation monitor - uses precise timers for status transitions
        tokio::spawn(
            super::monitors::reservation_monitor_task(
                Arc::clone(&shared_state),
                Arc::clone(&event_bus),
                event_bus.subscribe(),
            )
            .instrument(tracing::info_span!("reservation_monitor_task")),
        ),
        // Metrics updater - updates metrics every 5s
        #[cfg(feature = "metrics")]
        tokio::spawn(
            super::monitors::metrics_updater_task(Arc::clone(&shared_state))
                .instrument(tracing::info_span!("metrics_updater_task")),
        ),
    ];

    // Wait for all handlers (they run forever)
    for handle in handles {
        if let Err(e) = handle.await {
            tracing::error!(error = ?e, "Event handler task panicked");
        }
    }
}

/// Scheduler trigger handler with debouncing
async fn scheduler_trigger_handler_with_debounce(
    mut events: tokio::sync::broadcast::Receiver<EventEnvelope>,
    state: SharedState,
    event_bus: Arc<EventBus>,
) {
    let mut debounce = tokio::time::interval(Duration::from_millis(100));
    let mut pending_schedule = false;

    loop {
        tokio::select! {
            result = events.recv() => {
                match result {
                    Ok(event) => {
                        let handling_span = event.handling_span("scheduler_trigger_handler");
                        let _entered = handling_span.enter();
                        match event.event {
                            SchedulerEvent::JobSubmitted { .. }
                            | SchedulerEvent::JobUpdated { .. }
                            | SchedulerEvent::JobCompleted { .. }
                            | SchedulerEvent::GpuAvailabilityChanged { .. }
                            | SchedulerEvent::MemoryAvailabilityChanged { .. } => {
                                pending_schedule = true;
                            }
                            _ => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "Scheduler trigger handler lagged");
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
                    trigger_scheduling(&state, &event_bus).await;
                    pending_schedule = false;
                }
            }
        }
    }
}

/// Trigger job scheduling
async fn trigger_scheduling(state: &SharedState, event_bus: &Arc<EventBus>) {
    let scheduling_span = tracing::info_span!("trigger_scheduling");
    let _entered = scheduling_span.enter();
    #[cfg(feature = "metrics")]
    let started_at = std::time::Instant::now();

    // Step 1: Prepare jobs for execution (write lock - fast, no I/O)
    let jobs_to_execute = {
        let mut state_guard = state.write().await;
        let jobs = state_guard.scheduler.prepare_jobs_for_execution();

        // CRITICAL: Immediately refresh GPU slots after allocation to prevent race condition
        // This ensures that if another scheduling trigger happens before the periodic
        // GPU monitor runs, it will see the updated GPU availability
        if !jobs.is_empty() {
            state_guard.refresh_gpu_slots();
            // prepare_jobs_for_execution mutates job state/resources, so we must persist
            state_guard.mark_dirty();
        }

        jobs
    }; // Lock released here

    if jobs_to_execute.is_empty() {
        #[cfg(feature = "metrics")]
        gflow::metrics::observe_scheduler_latency("trigger_scheduling", started_at.elapsed());
        return;
    }

    tracing::info!(
        job_count = jobs_to_execute.len(),
        "Prepared jobs for execution"
    );

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
                .scheduler
                .get_job_runtime(job.id)
                .map(|rt| rt.state == JobState::Running)
                .unwrap_or(false)
        };

        if !should_execute {
            tracing::info!(
                job_id = job.id,
                "Skipping execution because state changed before execution"
            );
            execution_results.push((
                job.id,
                Err("Job state changed before execution".to_string()),
            ));
            continue;
        }

        match executor.execute(job) {
            Ok(_) => {
                tracing::info!(job_id = job.id, "Executed job");
                execution_results.push((job.id, Ok(())));
            }
            Err(e) => {
                tracing::error!(job_id = job.id, error = ?e, "Failed to execute job");
                execution_results.push((job.id, Err(e.to_string())));
            }
        }
    }

    // Step 3: Handle failures (write lock - brief)
    if !execution_results.is_empty() {
        let had_execution_errors = execution_results.iter().any(|(_, result)| result.is_err());
        let failure_outcomes = {
            let mut state_guard = state.write().await;
            let outcomes = state_guard
                .scheduler
                .handle_execution_failures_with_outcomes(&execution_results);
            state_guard.mark_dirty();
            outcomes
        };

        for outcome in &failure_outcomes {
            match outcome {
                ExecutionFailureOutcome::Retried {
                    job_id,
                    retry_attempt,
                    run_name,
                } => {
                    SchedulerRuntime::preserve_failed_tmux_session_for_retry(
                        *job_id,
                        run_name.as_deref(),
                        *retry_attempt,
                    );
                }
                ExecutionFailureOutcome::Failed {
                    job_id, run_name, ..
                } => {
                    SchedulerRuntime::cleanup_failed_tmux_session(*job_id, run_name.as_deref());
                }
            }
        }

        if had_execution_errors {
            let mut state_guard = state.write().await;
            state_guard.refresh_gpu_slots();
        }

        for outcome in failure_outcomes {
            match outcome {
                ExecutionFailureOutcome::Retried { job_id, .. } => {
                    event_bus.publish(SchedulerEvent::JobUpdated { job_id });
                }
                ExecutionFailureOutcome::Failed {
                    job_id,
                    gpu_ids,
                    memory_mb,
                    ..
                } => {
                    event_bus.publish(SchedulerEvent::JobCompleted {
                        job_id,
                        final_state: JobState::Failed,
                        gpu_ids,
                        memory_mb,
                    });
                }
            }
        }
    }

    #[cfg(feature = "metrics")]
    gflow::metrics::observe_scheduler_latency("trigger_scheduling", started_at.elapsed());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::executor::Executor;
    use std::process::Command;

    struct PartialTmuxFailExecutor;

    impl Executor for PartialTmuxFailExecutor {
        fn execute(&self, job: &Job) -> anyhow::Result<()> {
            let session_name = job
                .run_name
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing run_name"))?;
            let _session = gflow::tmux::TmuxSession::create(session_name.to_string())?;
            anyhow::bail!("simulated startup failure after tmux session creation")
        }
    }

    fn tmux_usable() -> bool {
        Command::new("tmux")
            .arg("start-server")
            .status()
            .is_ok_and(|status| status.success())
    }

    fn unique_run_name(prefix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{prefix}-{nanos}")
    }

    #[tokio::test]
    async fn startup_failures_publish_job_updated_and_preserve_tmux_session() {
        if !tmux_usable() {
            eprintln!(
                "Skipping startup_failures_publish_job_updated_and_preserve_tmux_session: tmux not usable"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(RwLock::new(
            SchedulerRuntime::with_state_path(
                Box::new(PartialTmuxFailExecutor),
                dir.path().to_path_buf(),
                None,
                gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
                gflow::config::ProjectsConfig::default(),
            )
            .unwrap(),
        ));
        let event_bus = Arc::new(EventBus::new(16));
        let mut events = event_bus.subscribe();

        let run_name = {
            let mut state_guard = state.write().await;
            let job = Job::builder()
                .command("echo test")
                .submitted_by("alice")
                .run_name(Some(unique_run_name("startup-retry")))
                .max_retry(Some(1))
                .build();
            let (_job_id, run_name, _job) = state_guard.submit_job(job).await.unwrap();
            run_name
        };

        trigger_scheduling(&state, &event_bus).await;

        let retried_run_name = gflow::tmux::retry_session_name(&run_name, 1);

        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .unwrap()
            .unwrap();
        match event.event {
            SchedulerEvent::JobUpdated { job_id } => assert_eq!(job_id, 1),
            other => panic!("unexpected event: {other:?}"),
        }

        let state_guard = state.read().await;
        let job = state_guard.get_job(1).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.retry_attempt, 1);
        drop(state_guard);

        assert!(!gflow::tmux::is_session_exist(&run_name));
        assert!(gflow::tmux::is_session_exist(&retried_run_name));
        let _ = gflow::tmux::kill_session(&retried_run_name);
    }
}
