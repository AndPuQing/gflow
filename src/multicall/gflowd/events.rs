//! Event-driven architecture for the gflow scheduler
//!
//! This module provides an event bus and event types for coordinating
//! scheduler operations without polling. Events are published when state
//! changes occur and handlers react to these events.

use gflow::core::job::{GpuIds, JobState, JobStateReason};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::Span;

/// Events that can occur in the scheduler
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some variants/fields are reserved for future use
pub enum SchedulerEvent {
    /// A job's state has changed
    JobStateChanged {
        job_id: u32,
        old_state: JobState,
        new_state: JobState,
        reason: Option<JobStateReason>,
    },

    /// A new job was submitted
    JobSubmitted { job_id: u32 },

    /// A job's parameters were updated
    JobUpdated { job_id: u32 },

    /// A job has completed (finished, failed, cancelled, or timed out)
    JobCompleted {
        job_id: u32,
        final_state: JobState,
        gpu_ids: Option<GpuIds>,
        memory_mb: Option<u64>,
    },

    /// GPU availability has changed
    GpuAvailabilityChanged { gpu_index: u32, available: bool },

    /// Memory has been freed
    MemoryAvailabilityChanged { freed_mb: u64 },

    /// A job has exceeded its time limit
    JobTimedOut {
        job_id: u32,
        run_name: Option<String>,
    },

    /// A zombie job was detected (tmux session disappeared)
    ZombieJobDetected { job_id: u32 },

    /// Periodic health check trigger
    PeriodicHealthCheck,

    /// A GPU reservation was created
    ReservationCreated { reservation_id: u32 },

    /// A GPU reservation was cancelled
    ReservationCancelled { reservation_id: u32 },

    /// The daemon has started (or reloaded)
    DaemonStarted,
}

impl SchedulerEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::JobStateChanged { .. } => "job_state_changed",
            Self::JobSubmitted { .. } => "job_submitted",
            Self::JobUpdated { .. } => "job_updated",
            Self::JobCompleted { .. } => "job_completed",
            Self::GpuAvailabilityChanged { .. } => "gpu_availability_changed",
            Self::MemoryAvailabilityChanged { .. } => "memory_availability_changed",
            Self::JobTimedOut { .. } => "job_timed_out",
            Self::ZombieJobDetected { .. } => "zombie_job_detected",
            Self::PeriodicHealthCheck => "periodic_health_check",
            Self::ReservationCreated { .. } => "reservation_created",
            Self::ReservationCancelled { .. } => "reservation_cancelled",
            Self::DaemonStarted => "daemon_started",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventEnvelope {
    pub event: SchedulerEvent,
    pub span: Span,
}

impl EventEnvelope {
    pub fn handling_span(&self, handler: &'static str) -> Span {
        tracing::info_span!(
            parent: &self.span,
            "scheduler_event",
            handler = handler,
            event_type = self.event.name()
        )
    }
}

/// Event bus for publishing and subscribing to scheduler events
#[derive(Clone)]
pub struct EventBus {
    sender: Arc<broadcast::Sender<EventEnvelope>>,
}

impl EventBus {
    /// Create a new event bus with the specified capacity
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender: Arc::new(sender),
        }
    }

    /// Publish an event to all subscribers
    pub fn publish(&self, event: SchedulerEvent) {
        let event_name = event.name();
        let subscriber_count = self.subscriber_count();
        let envelope = EventEnvelope {
            event,
            span: Span::current(),
        };
        tracing::debug!(
            event_type = event_name,
            subscriber_count,
            "Publishing scheduler event"
        );
        // Ignore send errors (no subscribers is fine)
        let _ = self.sender.send(envelope);
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers
    #[allow(dead_code)] // Useful for debugging/monitoring
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(100);
        let mut rx = bus.subscribe();

        // Publish an event
        bus.publish(SchedulerEvent::JobSubmitted { job_id: 1 });

        // Receive the event
        let event = rx.recv().await.unwrap();
        match event.event {
            SchedulerEvent::JobSubmitted { job_id } => assert_eq!(job_id, 1),
            _ => panic!("Unexpected event type"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(100);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        // Publish an event
        bus.publish(SchedulerEvent::JobSubmitted { job_id: 42 });

        // Both subscribers should receive the event
        let event1 = rx1.recv().await.unwrap();
        let event2 = rx2.recv().await.unwrap();

        match (event1.event, event2.event) {
            (
                SchedulerEvent::JobSubmitted { job_id: id1 },
                SchedulerEvent::JobSubmitted { job_id: id2 },
            ) => {
                assert_eq!(id1, 42);
                assert_eq!(id2, 42);
            }
            _ => panic!("Unexpected event types"),
        }
    }

    #[tokio::test]
    async fn test_subscriber_count() {
        let bus = EventBus::new(100);
        assert_eq!(bus.subscriber_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        drop(_rx1);
        // Note: subscriber count may not update immediately after drop
        // This is a limitation of the broadcast channel implementation
    }

    #[tokio::test]
    async fn test_no_subscribers_ok() {
        let bus = EventBus::new(100);
        // Publishing without subscribers should not panic
        bus.publish(SchedulerEvent::JobSubmitted { job_id: 1 });
    }
}
