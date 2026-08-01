//! Server-Sent Events stream for real-time dashboard updates.
//!
//! The stream subscribes to the daemon's [`EventBus`] and forwards scheduler
//! events as JSON payloads. Consumers (e.g. the web dashboard) re-fetch the
//! regular REST endpoints when an event arrives; the payloads are hints about
//! *what* changed, not full snapshots.

use super::super::state::ServerState;
use crate::multicall::gflowd::events::SchedulerEvent;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::{Stream, StreamExt as _};

pub(in crate::multicall::gflowd::server) async fn events_stream(
    State(server_state): State<ServerState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let receiver = server_state.event_bus.subscribe();

    let scheduler_events =
        tokio_stream::wrappers::BroadcastStream::new(receiver).filter_map(|envelope| {
            // A lagged subscriber misses events; that is fine because consumers
            // treat every event as a "re-sync now" hint rather than a delta.
            let envelope = envelope.ok()?;
            let (name, data) = event_payload(&envelope.event)?;
            let event = Event::default()
                .event(name)
                .json_data(data)
                .map_err(|error| {
                    tracing::warn!(%error, event_type = name, "Failed to serialize SSE payload");
                })
                .ok()?;
            Some(Ok(event))
        });

    // Emit an initial event so clients can confirm the stream is live without
    // waiting for the first scheduler activity or keep-alive tick.
    let connected = tokio_stream::once(Ok(Event::default().event("connected").data("{}")));

    Sse::new(connected.chain(scheduler_events)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Map a scheduler event to its SSE event name and JSON payload.
///
/// Returns `None` for events that carry no useful signal for external
/// consumers (e.g. periodic health checks).
fn event_payload(event: &SchedulerEvent) -> Option<(&'static str, serde_json::Value)> {
    use SchedulerEvent::*;

    // Every payload carries a `type` field mirroring the SSE event name so
    // consumers using a single generic message handler can still dispatch.
    let mut data = match event {
        JobStateChanged {
            job_id,
            old_state,
            new_state,
            reason,
        } => serde_json::json!({
            "job_id": job_id,
            "old_state": old_state,
            "new_state": new_state,
            "reason": reason,
        }),
        JobSubmitted { job_id } => serde_json::json!({ "job_id": job_id }),
        JobUpdated { job_id } => serde_json::json!({ "job_id": job_id }),
        JobCompleted {
            job_id,
            final_state,
            gpu_ids,
            memory_mb,
        } => serde_json::json!({
            "job_id": job_id,
            "final_state": final_state,
            "gpu_ids": gpu_ids,
            "memory_mb": memory_mb,
        }),
        GpuAvailabilityChanged {
            gpu_index,
            available,
        } => serde_json::json!({ "gpu_index": gpu_index, "available": available }),
        ManualGpuOverrideChanged {
            gpu_index,
            available,
        } => serde_json::json!({ "gpu_index": gpu_index, "available": available }),
        MemoryAvailabilityChanged { freed_mb } => serde_json::json!({ "freed_mb": freed_mb }),
        JobTimedOut { job_id, run_name } => {
            serde_json::json!({ "job_id": job_id, "run_name": run_name })
        }
        ZombieJobDetected { job_id } => serde_json::json!({ "job_id": job_id }),
        PeriodicHealthCheck => return None,
        ReservationCreated { reservation_id } => {
            serde_json::json!({ "reservation_id": reservation_id })
        }
        ReservationCancelled { reservation_id } => {
            serde_json::json!({ "reservation_id": reservation_id })
        }
        DaemonStarted => serde_json::json!({}),
    };

    data["type"] = serde_json::Value::String(event.name().to_string());

    Some((event.name(), data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::JobState;

    #[test]
    fn periodic_health_check_is_filtered_out() {
        assert!(event_payload(&SchedulerEvent::PeriodicHealthCheck).is_none());
    }

    #[test]
    fn job_state_changed_payload_uses_event_name_and_fields() {
        let (name, data) = event_payload(&SchedulerEvent::JobStateChanged {
            job_id: 7,
            old_state: JobState::Queued,
            new_state: JobState::Running,
            reason: None,
        })
        .expect("payload should be present");

        assert_eq!(name, "job_state_changed");
        assert_eq!(data["type"], "job_state_changed");
        assert_eq!(data["job_id"], 7);
        assert_eq!(data["old_state"], "Queued");
        assert_eq!(data["new_state"], "Running");
        assert!(data["reason"].is_null());
    }

    #[test]
    fn daemon_started_payload_is_empty_object() {
        let (name, data) =
            event_payload(&SchedulerEvent::DaemonStarted).expect("payload should be present");

        assert_eq!(name, "daemon_started");
        assert_eq!(data, serde_json::json!({ "type": "daemon_started" }));
    }
}
