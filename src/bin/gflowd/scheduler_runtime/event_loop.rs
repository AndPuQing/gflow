use super::*;
use crate::events::{EventBus, SchedulerEvent};
use std::sync::Arc;

/// Event-driven scheduling loop
pub async fn run_event_driven(shared_state: SharedState, event_bus: Arc<EventBus>) {
    // Spawn all event handlers and monitors
    let handles = vec![
        // Cascade handler - reacts to job failures/cancellations
        tokio::spawn(cascade_handler(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // Scheduler trigger handler with debouncing
        tokio::spawn(scheduler_trigger_handler_with_debounce(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // GPU monitor - polls NVML every 10s
        tokio::spawn(super::monitors::gpu_monitor_task(
            Arc::clone(&shared_state),
            Arc::clone(&event_bus),
        )),
        // Zombie monitor - checks tmux every 30s
        tokio::spawn(super::monitors::zombie_monitor_task(
            Arc::clone(&shared_state),
            Arc::clone(&event_bus),
        )),
        // Zombie handler - reacts to zombie events
        tokio::spawn(super::monitors::zombie_handler_task(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // Timeout monitor - checks time limits every 10s
        tokio::spawn(super::monitors::timeout_monitor_task(
            Arc::clone(&shared_state),
            Arc::clone(&event_bus),
        )),
        // Timeout handler - reacts to timeout events
        tokio::spawn(super::monitors::timeout_handler_task(
            event_bus.subscribe(),
            Arc::clone(&shared_state),
        )),
        // Reservation monitor - uses precise timers for status transitions
        tokio::spawn(super::monitors::reservation_monitor_task(
            Arc::clone(&shared_state),
            Arc::clone(&event_bus),
            event_bus.subscribe(),
        )),
        // Metrics updater - updates metrics every 5s
        #[cfg(feature = "metrics")]
        tokio::spawn(super::monitors::metrics_updater_task(Arc::clone(
            &shared_state,
        ))),
    ];

    // Wait for all handlers (they run forever)
    for handle in handles {
        if let Err(e) = handle.await {
            tracing::error!("Event handler task panicked: {:?}", e);
        }
    }
}

/// Cascade handler - reacts to job failures/cancellations and triggers cascade cancellation
async fn cascade_handler(
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    loop {
        match events.recv().await {
            Ok(SchedulerEvent::JobCompleted {
                job_id,
                final_state,
                ..
            }) => {
                // Only trigger cascade for failed, cancelled, or timed out jobs
                if matches!(
                    final_state,
                    JobState::Failed | JobState::Cancelled | JobState::Timeout
                ) {
                    let mut state_guard = state.write().await;
                    let cancelled = state_guard.scheduler.auto_cancel_dependent_jobs(job_id);
                    if !cancelled.is_empty() {
                        tracing::info!(
                            "Auto-cancelled {} dependent jobs due to job {} (state: {:?}): {:?}",
                            cancelled.len(),
                            job_id,
                            final_state,
                            cancelled
                        );
                        state_guard.mark_dirty();
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Cascade handler lagged, skipped {} events", skipped);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Event bus closed, cascade handler exiting");
                break;
            }
            _ => {}
        }
    }
}

/// Scheduler trigger handler with debouncing
async fn scheduler_trigger_handler_with_debounce(
    mut events: tokio::sync::broadcast::Receiver<SchedulerEvent>,
    state: SharedState,
) {
    let mut debounce = tokio::time::interval(Duration::from_millis(100));
    let mut pending_schedule = false;

    loop {
        tokio::select! {
            result = events.recv() => {
                match result {
                    Ok(event) => {
                        match event {
                            SchedulerEvent::JobSubmitted { .. }
                            | SchedulerEvent::JobCompleted { .. }
                            | SchedulerEvent::GpuAvailabilityChanged { .. }
                            | SchedulerEvent::MemoryAvailabilityChanged { .. } => {
                                pending_schedule = true;
                            }
                            _ => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("Scheduler trigger handler lagged, skipped {} events", skipped);
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
                    trigger_scheduling(&state).await;
                    pending_schedule = false;
                }
            }
        }
    }
}

/// Trigger job scheduling
async fn trigger_scheduling(state: &SharedState) {
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
        return;
    }

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
                .get_job(job.id)
                .map(|current_job| current_job.state == JobState::Running)
                .unwrap_or(false)
        };

        if !should_execute {
            tracing::info!(
                "Skipping execution of job {} (state changed before execution)",
                job.id
            );
            execution_results.push((
                job.id,
                Err("Job state changed before execution".to_string()),
            ));
            continue;
        }

        match executor.execute(job) {
            Ok(_) => {
                tracing::info!("Executed job {}", job.id);
                execution_results.push((job.id, Ok(())));
            }
            Err(e) => {
                tracing::error!("Failed to execute job {}: {:?}", job.id, e);
                execution_results.push((job.id, Err(e.to_string())));
            }
        }
    }

    // Step 3: Handle failures (write lock - brief)
    if !execution_results.is_empty() {
        let mut state_guard = state.write().await;
        state_guard
            .scheduler
            .handle_execution_failures(&execution_results);
        state_guard.mark_dirty();
    }
}
