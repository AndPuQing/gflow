use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// Status of a GPU reservation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReservationStatus {
    /// Scheduled but not yet active
    Pending,
    /// Currently active
    Active,
    /// Ended naturally
    Completed,
    /// Cancelled by user
    Cancelled,
}

/// GPU specification for a reservation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuSpec {
    /// Reserve a specific number of GPUs (scheduler will allocate dynamically)
    Count(u32),
    /// Reserve specific GPU indices (e.g., [0, 2, 3])
    Indices(Vec<u32>),
}

impl GpuSpec {
    /// Get the number of GPUs in this specification
    pub fn count(&self) -> u32 {
        match self {
            GpuSpec::Count(n) => *n,
            GpuSpec::Indices(indices) => indices.len() as u32,
        }
    }

    /// Get the GPU indices if this is an Indices spec, None otherwise
    pub fn indices(&self) -> Option<&[u32]> {
        match self {
            GpuSpec::Indices(indices) => Some(indices),
            GpuSpec::Count(_) => None,
        }
    }
}

/// A GPU reservation for a specific user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuReservation {
    /// Unique reservation ID
    pub id: u32,
    /// Username who created the reservation
    pub user: CompactString,
    /// GPU specification (count or specific indices)
    pub gpu_spec: GpuSpec,
    /// When reservation starts
    pub start_time: SystemTime,
    /// How long reservation lasts
    pub duration: Duration,
    /// Current status
    pub status: ReservationStatus,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Cancellation timestamp
    pub cancelled_at: Option<SystemTime>,
}

impl GpuReservation {
    /// Check if reservation is currently active based on current time
    pub fn is_active(&self, now: SystemTime) -> bool {
        if self.status == ReservationStatus::Cancelled {
            return false;
        }

        now >= self.start_time && now < self.end_time()
    }

    /// Calculate the end time of the reservation
    pub fn end_time(&self) -> SystemTime {
        self.start_time + self.duration
    }

    /// Check if this reservation overlaps with a given time range
    pub fn overlaps_with(&self, start: SystemTime, end: SystemTime) -> bool {
        // Two ranges overlap if: start1 < end2 AND start2 < end1
        self.start_time < end && start < self.end_time()
    }

    /// Update status based on current time
    pub fn update_status(&mut self, now: SystemTime) {
        match self.status {
            ReservationStatus::Pending => {
                if now >= self.start_time && now < self.end_time() {
                    self.status = ReservationStatus::Active;
                } else if now >= self.end_time() {
                    self.status = ReservationStatus::Completed;
                }
            }
            ReservationStatus::Active => {
                if now >= self.end_time() {
                    self.status = ReservationStatus::Completed;
                }
            }
            ReservationStatus::Completed | ReservationStatus::Cancelled => {
                // Terminal states, no change
            }
        }
    }

    /// Calculate the next status transition time for this reservation
    ///
    /// Returns `None` if the reservation is in a terminal state (Completed/Cancelled)
    /// or if the transition time is in the past.
    pub fn next_transition_time(&self, now: SystemTime) -> Option<SystemTime> {
        match self.status {
            ReservationStatus::Pending => {
                // Next transition: start_time (Pending → Active)
                if self.start_time > now {
                    Some(self.start_time)
                } else {
                    // Already past start time, should transition immediately
                    None
                }
            }
            ReservationStatus::Active => {
                // Next transition: end_time (Active → Completed)
                let end = self.end_time();
                if end > now {
                    Some(end)
                } else {
                    // Already past end time, should transition immediately
                    None
                }
            }
            ReservationStatus::Completed | ReservationStatus::Cancelled => {
                // Terminal states, no future transitions
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_active() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let duration = Duration::from_secs(3600); // 1 hour

        let mut reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time: start,
            duration,
            status: ReservationStatus::Pending,
            created_at: SystemTime::UNIX_EPOCH,
            cancelled_at: None,
        };

        // Before start time
        let before = start - Duration::from_secs(100);
        assert!(!reservation.is_active(before));

        // At start time
        assert!(reservation.is_active(start));

        // During reservation
        let during = start + Duration::from_secs(1800); // 30 minutes in
        assert!(reservation.is_active(during));

        // At end time
        let end = start + duration;
        assert!(!reservation.is_active(end));

        // After end time
        let after = end + Duration::from_secs(100);
        assert!(!reservation.is_active(after));

        // Cancelled reservation is never active
        reservation.status = ReservationStatus::Cancelled;
        assert!(!reservation.is_active(during));
    }

    #[test]
    fn test_end_time() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let duration = Duration::from_secs(3600);

        let reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time: start,
            duration,
            status: ReservationStatus::Pending,
            created_at: SystemTime::UNIX_EPOCH,
            cancelled_at: None,
        };

        assert_eq!(reservation.end_time(), start + duration);
    }

    #[test]
    fn test_overlaps_with() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let duration = Duration::from_secs(3600); // 1 hour

        let reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time: start,
            duration,
            status: ReservationStatus::Pending,
            created_at: SystemTime::UNIX_EPOCH,
            cancelled_at: None,
        };

        let end = start + duration;

        // Completely before
        let before_start = start - Duration::from_secs(200);
        let before_end = start - Duration::from_secs(100);
        assert!(!reservation.overlaps_with(before_start, before_end));

        // Completely after
        let after_start = end + Duration::from_secs(100);
        let after_end = end + Duration::from_secs(200);
        assert!(!reservation.overlaps_with(after_start, after_end));

        // Overlaps at start
        let overlap_start = start - Duration::from_secs(100);
        let overlap_end = start + Duration::from_secs(100);
        assert!(reservation.overlaps_with(overlap_start, overlap_end));

        // Overlaps at end
        let overlap_start = end - Duration::from_secs(100);
        let overlap_end = end + Duration::from_secs(100);
        assert!(reservation.overlaps_with(overlap_start, overlap_end));

        // Completely contains
        let contains_start = start - Duration::from_secs(100);
        let contains_end = end + Duration::from_secs(100);
        assert!(reservation.overlaps_with(contains_start, contains_end));

        // Completely contained
        let contained_start = start + Duration::from_secs(100);
        let contained_end = end - Duration::from_secs(100);
        assert!(reservation.overlaps_with(contained_start, contained_end));

        // Exact match
        assert!(reservation.overlaps_with(start, end));
    }

    #[test]
    fn test_update_status() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let duration = Duration::from_secs(3600);
        let end = start + duration;

        let mut reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time: start,
            duration,
            status: ReservationStatus::Pending,
            created_at: SystemTime::UNIX_EPOCH,
            cancelled_at: None,
        };

        // Before start: stays Pending
        let before = start - Duration::from_secs(100);
        reservation.update_status(before);
        assert_eq!(reservation.status, ReservationStatus::Pending);

        // At start: becomes Active
        reservation.update_status(start);
        assert_eq!(reservation.status, ReservationStatus::Active);

        // During: stays Active
        let during = start + Duration::from_secs(1800);
        reservation.update_status(during);
        assert_eq!(reservation.status, ReservationStatus::Active);

        // At end: becomes Completed
        reservation.update_status(end);
        assert_eq!(reservation.status, ReservationStatus::Completed);

        // After end: stays Completed
        let after = end + Duration::from_secs(100);
        reservation.update_status(after);
        assert_eq!(reservation.status, ReservationStatus::Completed);

        // Cancelled stays Cancelled
        reservation.status = ReservationStatus::Cancelled;
        reservation.update_status(during);
        assert_eq!(reservation.status, ReservationStatus::Cancelled);
    }

    #[test]
    fn test_pending_to_completed_directly() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let duration = Duration::from_secs(3600);
        let end = start + duration;

        let mut reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time: start,
            duration,
            status: ReservationStatus::Pending,
            created_at: SystemTime::UNIX_EPOCH,
            cancelled_at: None,
        };

        // If we check after end time while still Pending, it should go to Completed
        let after = end + Duration::from_secs(100);
        reservation.update_status(after);
        assert_eq!(reservation.status, ReservationStatus::Completed);
    }

    #[test]
    fn test_next_transition_time_pending() {
        let now = SystemTime::now();
        let start_time = now + Duration::from_secs(3600); // 1 hour from now

        let reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time,
            duration: Duration::from_secs(7200),
            status: ReservationStatus::Pending,
            created_at: now,
            cancelled_at: None,
        };

        // Should return start_time for pending reservation
        assert_eq!(reservation.next_transition_time(now), Some(start_time));

        // If current time is past start_time, should return None
        let future = start_time + Duration::from_secs(100);
        assert_eq!(reservation.next_transition_time(future), None);
    }

    #[test]
    fn test_next_transition_time_active() {
        let now = SystemTime::now();
        let start_time = now - Duration::from_secs(1800); // Started 30 min ago
        let duration = Duration::from_secs(3600); // 1 hour total
        let end_time = start_time + duration;

        let reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time,
            duration,
            status: ReservationStatus::Active,
            created_at: now - Duration::from_secs(2000),
            cancelled_at: None,
        };

        // Should return end_time for active reservation
        assert_eq!(reservation.next_transition_time(now), Some(end_time));

        // If current time is past end_time, should return None
        let future = end_time + Duration::from_secs(100);
        assert_eq!(reservation.next_transition_time(future), None);
    }

    #[test]
    fn test_next_transition_time_terminal_states() {
        let now = SystemTime::now();
        let start_time = now - Duration::from_secs(7200);

        let mut reservation = GpuReservation {
            id: 1,
            user: "alice".into(),
            gpu_spec: GpuSpec::Count(2),
            start_time,
            duration: Duration::from_secs(3600),
            status: ReservationStatus::Completed,
            created_at: now - Duration::from_secs(8000),
            cancelled_at: None,
        };

        // Completed reservation should return None
        assert_eq!(reservation.next_transition_time(now), None);

        // Cancelled reservation should return None
        reservation.status = ReservationStatus::Cancelled;
        reservation.cancelled_at = Some(now);
        assert_eq!(reservation.next_transition_time(now), None);
    }

    #[test]
    fn test_gpu_spec_count() {
        let spec = GpuSpec::Count(4);
        assert_eq!(spec.count(), 4);
        assert_eq!(spec.indices(), None);
    }

    #[test]
    fn test_gpu_spec_indices() {
        let spec = GpuSpec::Indices(vec![0, 2, 3]);
        assert_eq!(spec.count(), 3);
        assert_eq!(spec.indices(), Some(&[0, 2, 3][..]));
    }

    #[test]
    fn test_gpu_spec_empty_indices() {
        let spec = GpuSpec::Indices(vec![]);
        assert_eq!(spec.count(), 0);
        assert_eq!(spec.indices(), Some(&[][..]));
    }
}
