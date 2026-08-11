use super::super::events::{EventBus, EventEnvelope, SchedulerEvent};
use super::*;
use std::sync::Arc;

const ZOMBIE_STARTUP_GRACE_PERIOD: Duration = Duration::from_secs(30);

fn should_check_missing_session_as_zombie(
    started_at: Option<std::time::SystemTime>,
    now: std::time::SystemTime,
) -> bool {
    let Some(started_at) = started_at else {
        // Legacy/recovered Running jobs may not have persisted `started_at`.
        // Keep checking them so they don't get stuck in Running forever.
        return true;
    };

    let Ok(elapsed) = now.duration_since(started_at) else {
        // Clock skew/backwards adjustments can make `started_at` appear in the future.
        // Keep checking so missing sessions still get recovered.
        return true;
    };

    elapsed >= ZOMBIE_STARTUP_GRACE_PERIOD
}

/// GPU monitor task - polls NVML on the configured interval and publishes changes
pub(super) async fn gpu_monitor_task(
    state: SharedState,
    event_bus: Arc<EventBus>,
    poll_interval: Duration,
) {
    let mut interval = tokio::time::interval(poll_interval);
    let mut previous_gpu_states: HashMap<u32, bool> = HashMap::new();

    loop {
        interval.tick().await;

        let info = {
            let mut state_guard = state.write().await;
            state_guard.refresh_gpu_slots();
            state_guard.info()
        };

        for gpu_info in &info.gpus {
            let previous_available = previous_gpu_states.get(&gpu_info.index).copied();
            if previous_available != Some(gpu_info.available) {
                event_bus.publish(SchedulerEvent::GpuAvailabilityChanged {
                    gpu_index: gpu_info.index,
                    available: gpu_info.available,
                });
                previous_gpu_states.insert(gpu_info.index, gpu_info.available);
            }
        }
    }
}

/// Zombie monitor task - probes job liveness through the configured executor
/// (process executor: real process liveness; tmux: session existence) every 10s.
pub(super) async fn zombie_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        // Collect running jobs and the executor handle (with read lock)
        let (running_jobs, executor) = {
            let state_guard = state.read().await;
            let executor = state_guard.executor();
            let jobs = state_guard
                .job_runtimes()
                .iter()
                .filter(|rt| rt.state == JobState::Running)
                .map(|rt| {
                    let run_name = state_guard
                        .scheduler
                        .get_job_spec(rt.id)
                        .and_then(|spec| spec.run_name.clone());
                    (rt.id, run_name, rt.started_at)
                })
                .collect::<Vec<_>>();
            (jobs, executor)
        };

        if running_jobs.is_empty() {
            continue;
        }

        let running_job_ids: HashSet<u32> = running_jobs.iter().map(|(id, _, _)| *id).collect();
        let finished_results: Vec<_> = executor
            .collect_finished()
            .into_iter()
            .filter(|result| running_job_ids.contains(&result.job_id))
            .collect();
        let finished_ids: HashSet<u32> = finished_results
            .iter()
            .map(|result| result.job_id)
            .collect();

        // Process completion results before applying the startup grace period:
        // a payload that exited successfully while the daemon was offline must
        // be finalized immediately after its result file becomes visible.
        for result in finished_results {
            tracing::info!(
                job_id = result.job_id,
                exit_code = ?result.exit_code,
                signal = ?result.signal,
                "Collected job runner exit result"
            );
            event_bus.publish(SchedulerEvent::JobExecutionFinished {
                job_id: result.job_id,
                exit_code: result.exit_code,
                signal: result.signal,
            });
        }

        // Sample time after building the Running-job snapshot so jobs that
        // started during snapshot construction don't look like future starts.
        let now = std::time::SystemTime::now();

        // Check which jobs are zombies (no lock held). A result is authoritative
        // even if the runner is in the short window before it exits.
        for (job_id, run_name, started_at) in running_jobs {
            if finished_ids.contains(&job_id)
                || !should_check_missing_session_as_zombie(started_at, now)
            {
                continue;
            }
            if !executor.is_running(job_id, run_name.as_deref()) {
                tracing::warn!(job_id, run_name = ?run_name, "Found zombie job");
                event_bus.publish(SchedulerEvent::ZombieJobDetected { job_id });
            }
        }
    }
}

/// Runner completion and zombie handler. A durable exit result is mapped to
/// Finished/Failed through SchedulerRuntime so resource accounting, retries,
/// dependencies, and executor cleanup all use the normal transition path.
pub(super) async fn zombie_handler_task(
    mut events: tokio::sync::broadcast::Receiver<EventEnvelope>,
    state: SharedState,
    event_bus: Arc<EventBus>,
) {
    loop {
        match events.recv().await {
            Ok(event) => {
                let handling_span = event.handling_span("execution_result_handler");
                let _entered = handling_span.enter();
                let result = match event.event {
                    SchedulerEvent::JobExecutionFinished {
                        job_id,
                        exit_code,
                        signal,
                    } => ExecutionResult {
                        job_id,
                        exit_code,
                        signal,
                    },
                    SchedulerEvent::ZombieJobDetected { job_id } => ExecutionResult {
                        job_id,
                        exit_code: None,
                        signal: None,
                    },
                    _ => continue,
                };

                finalize_execution_result(&state, &event_bus, result).await;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "Execution result handler lagged");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, execution result handler exiting");
                break;
            }
        }
    }
}

async fn finalize_execution_result(
    state: &SharedState,
    event_bus: &Arc<EventBus>,
    result: ExecutionResult,
) {
    let transition = {
        let mut state_guard = state.write().await;
        let Some(job) = state_guard.get_job(result.job_id) else {
            return;
        };
        let gpu_ids = job.gpu_ids.clone();
        let memory_mb = job.memory_limit_mb;

        if result.succeeded() {
            if state_guard.finish_job(result.job_id).await {
                Some((JobState::Finished, gpu_ids, memory_mb, None))
            } else {
                None
            }
        } else {
            state_guard
                .fail_job(result.job_id)
                .await
                .map(|retry_job_id| (JobState::Failed, gpu_ids, memory_mb, retry_job_id))
        }
    };

    if let Some((final_state, gpu_ids, memory_mb, retry_job_id)) = transition {
        event_bus.publish(SchedulerEvent::JobCompleted {
            job_id: result.job_id,
            final_state,
            gpu_ids,
            memory_mb,
        });
        if let Some(new_job_id) = retry_job_id {
            event_bus.publish(SchedulerEvent::JobSubmitted { job_id: new_job_id });
        }

        if final_state == JobState::Finished {
            tracing::info!(
                job_id = result.job_id,
                exit_code = ?result.exit_code,
                "Marked job finished from runner exit result"
            );
        } else {
            tracing::info!(
                job_id = result.job_id,
                exit_code = ?result.exit_code,
                signal = ?result.signal,
                "Marked job failed from runner exit result"
            );
        }
    }
}

/// Begin-time monitor task - releases queued jobs whose scheduled start time
/// (`--begin` / `scheduled_at`) has arrived.
///
/// Runs every 10s; the first tick fires immediately so jobs whose begin time
/// passed while the daemon was down are picked up right after startup.
pub(super) async fn begin_time_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        let released = {
            let mut state_guard = state.write().await;
            state_guard.release_due_scheduled_jobs()
        };

        if !released.is_empty() {
            tracing::info!(released = ?released, "Released scheduled jobs whose begin time arrived");
            // Run a scheduling pass so the released jobs can start immediately
            // (the direct call avoids a fake "job submitted" event, which
            // would fire webhooks/notifications for already-submitted jobs).
            super::event_loop::trigger_scheduling(&state, &event_bus).await;
        }
    }
}

/// Timeout monitor task - checks time limits every 10s
pub(super) async fn timeout_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        // Check for timed-out jobs (read lock)
        let timed_out_jobs = {
            let state_guard = state.read().await;
            let now = std::time::SystemTime::now();
            state_guard
                .job_runtimes()
                .iter()
                .filter(|rt| rt.state == JobState::Running)
                .filter_map(|rt| {
                    let (Some(time_limit), Some(started_at)) = (rt.time_limit, rt.started_at)
                    else {
                        return None;
                    };

                    let Ok(elapsed) = now.duration_since(started_at) else {
                        return None;
                    };

                    if elapsed > time_limit {
                        let run_name = state_guard
                            .scheduler
                            .get_job_spec(rt.id)
                            .and_then(|spec| spec.run_name.as_ref().map(|s| s.to_string()));
                        tracing::warn!(job_id = rt.id, "Job exceeded time limit");
                        Some((rt.id, run_name))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };

        // Publish timeout events
        for (job_id, run_name) in timed_out_jobs {
            event_bus.publish(SchedulerEvent::JobTimedOut { job_id, run_name });
        }
    }
}

/// Timeout handler task - reacts to timeout events and terminates jobs
pub(super) async fn timeout_handler_task(
    mut events: tokio::sync::broadcast::Receiver<EventEnvelope>,
    state: SharedState,
    event_bus: Arc<EventBus>,
) {
    loop {
        match events.recv().await {
            Ok(event) => {
                let handling_span = event.handling_span("timeout_handler");
                let _entered = handling_span.enter();
                let SchedulerEvent::JobTimedOut { job_id, run_name } = event.event else {
                    continue;
                };
                // Terminate the job's execution (no lock held): process executor
                // SIGTERMs the process group, tmux sends Ctrl-C.
                let executor = { state.read().await.executor() };
                if let Err(e) = executor.terminate(job_id, run_name.as_deref()) {
                    tracing::error!(job_id, error = %e, "Failed to terminate timed-out job");
                }

                // Update job state (write lock)
                let result = {
                    let mut state_guard = state.write().await;
                    state_guard.timeout_job(job_id).await
                };

                if let Some(Some(new_job_id)) = result {
                    event_bus.publish(SchedulerEvent::JobSubmitted { job_id: new_job_id });
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "Timeout handler lagged");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, timeout handler exiting");
                break;
            }
        }
    }
}

/// Metrics updater task - updates metrics every 5s
#[cfg(feature = "metrics")]
pub(super) async fn metrics_updater_task(state: SharedState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        let state_guard = state.read().await;

        // Update job state metrics
        gflow::metrics::update_job_state_metrics_runtimes(state_guard.job_runtimes());

        // Update GPU metrics
        let info = state_guard.info();
        let available_gpus = info.gpus.iter().filter(|g| g.available).count();
        let total_gpus = info.gpus.len();
        gflow::metrics::update_resource_metrics(
            available_gpus,
            total_gpus,
            state_guard.available_memory_mb(),
            state_guard.total_memory_mb(),
        );
    }
}

/// Reservation monitor task - uses precise timers for status transitions
pub(super) async fn reservation_monitor_task(
    state: SharedState,
    event_bus: Arc<EventBus>,
    mut events: tokio::sync::broadcast::Receiver<EventEnvelope>,
) {
    // CRITICAL: On startup/reload, immediately update reservation statuses
    // to handle any transitions that occurred while gflowd was down
    {
        let mut state_guard = state.write().await;
        let before_count = state_guard.scheduler.reservations.len();
        state_guard.scheduler.update_reservation_statuses();
        let after_count = state_guard.scheduler.reservations.len();

        if before_count != after_count {
            tracing::info!(
                "Startup: Updated reservation statuses ({} -> {} active reservations)",
                before_count,
                after_count
            );
            state_guard.mark_dirty();
        }
        drop(state_guard);

        // Trigger scheduling in case reservations changed
        event_bus.publish(SchedulerEvent::PeriodicHealthCheck);
    }

    loop {
        // Calculate next transition time
        let next_transition = {
            let state_guard = state.read().await;
            calculate_next_reservation_transition(&state_guard.scheduler.reservations)
        };

        match next_transition {
            Some(deadline) => {
                // Convert SystemTime to Instant for tokio
                let now = std::time::SystemTime::now();
                let sleep_duration = deadline
                    .duration_since(now)
                    .unwrap_or(Duration::from_secs(0));

                // Wait until the next transition or a reservation change event
                tokio::select! {
                    _ = tokio::time::sleep(sleep_duration) => {
                        // Transition time reached, update statuses
                        let mut state_guard = state.write().await;
                        state_guard.scheduler.update_reservation_statuses();
                        drop(state_guard);
                        event_bus.publish(SchedulerEvent::PeriodicHealthCheck);
                    }
                    result = events.recv() => {
                        match result {
                            Ok(event) => {
                                let handling_span = event.handling_span("reservation_monitor");
                                let _entered = handling_span.enter();
                                match event.event {
                                    SchedulerEvent::ReservationCreated { .. } | SchedulerEvent::ReservationCancelled { .. } => {
                                        // Reservation list changed, recalculate next transition
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::info!("Event bus closed, reservation monitor exiting");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            None => {
                // No reservations, wait for a new one to be created
                match events.recv().await {
                    Ok(event) => {
                        let handling_span = event.handling_span("reservation_monitor");
                        let _entered = handling_span.enter();
                        if matches!(event.event, SchedulerEvent::ReservationCreated { .. }) {
                            // New reservation added, recalculate
                            continue;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("Event bus closed, reservation monitor exiting");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Calculate the next reservation status transition time
fn calculate_next_reservation_transition(
    reservations: &[gflow::core::reservation::GpuReservation],
) -> Option<std::time::SystemTime> {
    let now = std::time::SystemTime::now();

    reservations
        .iter()
        .filter_map(|r| r.next_transition_time(now))
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::reservation::{GpuReservation, GpuSpec, ReservationStatus};
    use std::time::{Duration, SystemTime};

    #[test]
    fn zombie_check_allows_legacy_jobs_without_start_time() {
        let now = SystemTime::now();
        assert!(should_check_missing_session_as_zombie(None, now));
    }

    #[test]
    fn zombie_check_skips_recently_started_jobs() {
        let now = SystemTime::now();
        let started_at = now.checked_sub(Duration::from_secs(5));
        assert!(!should_check_missing_session_as_zombie(started_at, now));
    }

    #[test]
    fn zombie_check_allows_old_running_jobs() {
        let now = SystemTime::now();
        let started_at = now.checked_sub(Duration::from_secs(45));
        assert!(should_check_missing_session_as_zombie(started_at, now));
    }

    #[test]
    fn zombie_check_allows_future_started_at_jobs() {
        let now = SystemTime::now();
        let started_at = now.checked_add(Duration::from_secs(45));
        assert!(should_check_missing_session_as_zombie(started_at, now));
    }

    #[test]
    fn test_calculate_next_transition_no_reservations() {
        let reservations = vec![];
        let result = calculate_next_reservation_transition(&reservations);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_next_transition_pending_reservation() {
        let now = SystemTime::now();
        let start_time = now + Duration::from_secs(3600); // 1 hour from now

        let reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time,
            duration: Duration::from_secs(7200), // 2 hours
            status: ReservationStatus::Pending,
            created_at: now,
            cancelled_at: None,
        };

        let result = calculate_next_reservation_transition(&[reservation]);
        assert_eq!(result, Some(start_time));
    }

    #[test]
    fn test_calculate_next_transition_active_reservation() {
        let now = SystemTime::now();
        let start_time = now - Duration::from_secs(1800); // Started 30 min ago
        let duration = Duration::from_secs(3600); // 1 hour total
        let end_time = start_time + duration;

        let mut reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time,
            duration,
            status: ReservationStatus::Active,
            created_at: now - Duration::from_secs(2000),
            cancelled_at: None,
        };

        let result = calculate_next_reservation_transition(&[reservation.clone()]);
        assert_eq!(result, Some(end_time));

        // Test with completed reservation (should be ignored)
        reservation.status = ReservationStatus::Completed;
        let result = calculate_next_reservation_transition(&[reservation]);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_next_transition_multiple_reservations() {
        let now = SystemTime::now();
        let start1 = now + Duration::from_secs(3600); // 1 hour from now
        let start2 = now + Duration::from_secs(1800); // 30 min from now (earlier)
        let start3 = now + Duration::from_secs(7200); // 2 hours from now

        let reservations = vec![
            GpuReservation {
                id: 1,
                user: "alice".into(),
                gpu_spec: GpuSpec::Count(2),
                start_time: start1,
                duration: Duration::from_secs(3600),
                status: ReservationStatus::Pending,
                created_at: now,
                cancelled_at: None,
            },
            GpuReservation {
                id: 2,
                user: "bob".into(),
                gpu_spec: GpuSpec::Count(1),
                start_time: start2,
                duration: Duration::from_secs(3600),
                status: ReservationStatus::Pending,
                created_at: now,
                cancelled_at: None,
            },
            GpuReservation {
                id: 3,
                user: "charlie".into(),
                gpu_spec: GpuSpec::Count(1),
                start_time: start3,
                duration: Duration::from_secs(3600),
                status: ReservationStatus::Pending,
                created_at: now,
                cancelled_at: None,
            },
        ];

        let result = calculate_next_reservation_transition(&reservations);
        // Should return the earliest transition time (start2)
        assert_eq!(result, Some(start2));
    }

    #[test]
    fn test_calculate_next_transition_ignores_past_times() {
        let now = SystemTime::now();
        let past_time = now - Duration::from_secs(3600); // 1 hour ago
        let future_time = now + Duration::from_secs(3600); // 1 hour from now

        let reservations = vec![
            GpuReservation {
                id: 1,
                user: "alice".into(),
                gpu_spec: GpuSpec::Count(2),
                start_time: past_time,
                duration: Duration::from_secs(1800),
                status: ReservationStatus::Pending,
                created_at: now - Duration::from_secs(7200),
                cancelled_at: None,
            },
            GpuReservation {
                id: 2,
                user: "bob".into(),
                gpu_spec: GpuSpec::Count(1),
                start_time: future_time,
                duration: Duration::from_secs(3600),
                status: ReservationStatus::Pending,
                created_at: now,
                cancelled_at: None,
            },
        ];

        let result = calculate_next_reservation_transition(&reservations);
        // Should ignore past time and return future_time
        assert_eq!(result, Some(future_time));
    }

    #[test]
    fn test_calculate_next_transition_cancelled_ignored() {
        let now = SystemTime::now();
        let start_time = now + Duration::from_secs(3600);

        let reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time,
            duration: Duration::from_secs(3600),
            status: ReservationStatus::Cancelled,
            created_at: now,
            cancelled_at: Some(now),
        };

        let result = calculate_next_reservation_transition(&[reservation]);
        assert!(result.is_none());
    }
}
