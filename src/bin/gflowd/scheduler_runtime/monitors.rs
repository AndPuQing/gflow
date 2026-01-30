use super::*;
use crate::events::{EventBus, SchedulerEvent};
use gflow::tmux::disable_pipe_pane_for_job;
use std::sync::Arc;

/// GPU monitor task - polls NVML every 10s and publishes changes
pub(super) async fn gpu_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    let mut previous_gpu_states: HashMap<u32, bool> = HashMap::new();

    loop {
        interval.tick().await;

        // Refresh GPU slots
        {
            let mut state_guard = state.write().await;
            state_guard.refresh_gpu_slots();
        }

        // Check for changes and publish events
        let state_guard = state.read().await;
        let info = state_guard.info();
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

/// Zombie monitor task - checks tmux sessions every 10s
pub(super) async fn zombie_monitor_task(state: SharedState, event_bus: Arc<EventBus>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        // Collect running jobs (with read lock)
        let running_jobs = {
            let state_guard = state.read().await;
            state_guard
                .jobs()
                .iter()
                .filter(|j| j.state == JobState::Running)
                .map(|j| (j.id, j.run_name.clone()))
                .collect::<Vec<_>>()
        };

        if running_jobs.is_empty() {
            continue;
        }

        // Get all tmux sessions in a single batch call (no lock held)
        let existing_sessions = gflow::tmux::get_all_session_names();

        // Check which jobs are zombies
        for (job_id, run_name) in running_jobs {
            if let Some(rn) = run_name {
                if !existing_sessions.contains(rn.as_str()) {
                    tracing::warn!("Found zombie job (id: {}), publishing event", job_id);
                    event_bus.publish(SchedulerEvent::ZombieJobDetected { job_id });
                }
            }
        }
    }
}

/// Zombie handler task - reacts to zombie events and marks jobs as failed
pub(super) async fn zombie_handler_task(
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    loop {
        match events.recv().await {
            Ok(SchedulerEvent::ZombieJobDetected { job_id }) => {
                // Get run_name before acquiring write lock
                let run_name = {
                    let state_guard = state.read().await;
                    state_guard
                        .scheduler
                        .get_job(job_id)
                        .and_then(|j| j.run_name.clone())
                };

                // Update job state (write lock)
                let mut state_guard = state.write().await;
                if let Some(job) = state_guard.scheduler.get_job_mut(job_id) {
                    job.state = JobState::Failed;
                    job.finished_at = Some(std::time::SystemTime::now());
                    state_guard.mark_dirty();
                    tracing::info!("Marked zombie job {} as Failed", job_id);
                }
                drop(state_guard); // Release lock before disabling PipePane

                // Disable PipePane if session still exists (no lock held)
                // This handles the case where the session was manually killed but PipePane might still be active
                if let Some(rn) = run_name {
                    disable_pipe_pane_for_job(job_id, &rn, true);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Zombie handler lagged, skipped {} events", skipped);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, zombie handler exiting");
                break;
            }
            _ => {}
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
            state_guard
                .jobs()
                .iter()
                .filter(|job| job.has_exceeded_time_limit())
                .map(|job| {
                    tracing::warn!("Job {} has exceeded time limit, publishing event", job.id);
                    (job.id, job.run_name.as_ref().map(|s| s.to_string()))
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
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    loop {
        match events.recv().await {
            Ok(SchedulerEvent::JobTimedOut { job_id, run_name }) => {
                // Send Ctrl-C to terminate the job (no lock held)
                if let Some(rn) = &run_name {
                    if let Err(e) = gflow::tmux::send_ctrl_c(rn) {
                        tracing::error!("Failed to send C-c to timed-out job {}: {}", job_id, e);
                    }
                }

                // Update job state (write lock)
                let mut state_guard = state.write().await;
                if let Some(job) = state_guard.scheduler.get_job_mut(job_id) {
                    job.try_transition(job_id, JobState::Timeout);

                    // Auto-cancel dependent jobs
                    let cancelled = state_guard.scheduler.auto_cancel_dependent_jobs(job_id);
                    if !cancelled.is_empty() {
                        tracing::info!(
                            "Auto-cancelled {} dependent jobs due to timeout of job {}: {:?}",
                            cancelled.len(),
                            job_id,
                            cancelled
                        );
                    }

                    state_guard.mark_dirty();
                }
                drop(state_guard); // Release lock before disabling PipePane

                // Disable PipePane to prevent process leaks (no lock held)
                if let Some(rn) = run_name {
                    disable_pipe_pane_for_job(job_id, &rn, false);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Timeout handler lagged, skipped {} events", skipped);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, timeout handler exiting");
                break;
            }
            _ => {}
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
        gflow::metrics::update_job_state_metrics(state_guard.jobs());

        // Update GPU metrics
        let info = state_guard.info();
        let available_gpus = info.gpus.iter().filter(|g| g.available).count();
        let total_gpus = info.gpus.len();
        gflow::metrics::GPU_AVAILABLE
            .with_label_values(&[] as &[&str])
            .set(available_gpus as f64);
        gflow::metrics::GPU_TOTAL
            .with_label_values(&[] as &[&str])
            .set(total_gpus as f64);

        // Update memory metrics
        gflow::metrics::MEMORY_AVAILABLE_MB
            .with_label_values(&[] as &[&str])
            .set(state_guard.available_memory_mb() as f64);
        gflow::metrics::MEMORY_TOTAL_MB
            .with_label_values(&[] as &[&str])
            .set(state_guard.total_memory_mb() as f64);
    }
}

/// Reservation monitor task - uses precise timers for status transitions
pub(super) async fn reservation_monitor_task(
    state: SharedState,
    event_bus: Arc<EventBus>,
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
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
                            Ok(SchedulerEvent::ReservationCreated { .. } | SchedulerEvent::ReservationCancelled { .. }) => {
                                // Reservation list changed, recalculate next transition
                                continue;
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
                    Ok(SchedulerEvent::ReservationCreated { .. }) => {
                        // New reservation added, recalculate
                        continue;
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
