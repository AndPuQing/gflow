//! Event-driven architecture for the gflow scheduler
//!
//! This module provides an event bus and event types for coordinating
//! scheduler operations without polling. Events are published when state
//! changes occur and handlers react to these events.

use gflow::core::job::{JobState, JobStateReason};
use std::sync::Arc;
use tokio::sync::broadcast;

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

    /// A job has completed (finished, failed, cancelled, or timed out)
    JobCompleted {
        job_id: u32,
        final_state: JobState,
        gpu_ids: Option<Vec<u32>>,
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
}

/// Event bus for publishing and subscribing to scheduler events
#[derive(Clone)]
pub struct EventBus {
    sender: Arc<broadcast::Sender<SchedulerEvent>>,
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
        // Ignore send errors (no subscribers is fine)
        let _ = self.sender.send(event);
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<SchedulerEvent> {
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
        match event {
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

        match (event1, event2) {
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
