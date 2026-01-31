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

    // Property-based tests
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        // Strategy to generate valid time ranges
        fn time_range_strategy() -> impl Strategy<Value = (SystemTime, Duration)> {
            (1000u64..1_000_000_000, 1u64..86400).prop_map(|(start_secs, duration_secs)| {
                let start = SystemTime::UNIX_EPOCH + Duration::from_secs(start_secs);
                let duration = Duration::from_secs(duration_secs);
                (start, duration)
            })
        }

        proptest! {
            /// Property: Overlap detection is symmetric
            /// If reservation A overlaps with B, then B overlaps with A
            #[test]
            fn prop_overlap_is_symmetric(
                (start1, dur1) in time_range_strategy(),
                (start2, dur2) in time_range_strategy(),
            ) {
                let res1 = GpuReservation {
                    id: 1,
                    user: "alice".into(),
                    gpu_spec: GpuSpec::Count(2),
                    start_time: start1,
                    duration: dur1,
                    status: ReservationStatus::Pending,
                    created_at: SystemTime::UNIX_EPOCH,
                    cancelled_at: None,
                };

                let end2 = start2 + dur2;
                let overlap_1_with_2 = res1.overlaps_with(start2, end2);

                // Create res2 and check reverse
                let res2 = GpuReservation {
                    id: 2,
                    user: "bob".into(),
                    gpu_spec: GpuSpec::Count(2),
                    start_time: start2,
                    duration: dur2,
                    status: ReservationStatus::Pending,
                    created_at: SystemTime::UNIX_EPOCH,
                    cancelled_at: None,
                };

                let end1 = start1 + dur1;
                let overlap_2_with_1 = res2.overlaps_with(start1, end1);

                prop_assert_eq!(overlap_1_with_2, overlap_2_with_1);
            }

            /// Property: A reservation never overlaps with itself at non-overlapping times
            /// If we have a reservation [start, end), it should not overlap with [end+1, end+2)
            #[test]
            fn prop_no_overlap_after_end(
                (start, dur) in time_range_strategy(),
                gap in 1u64..1000,
            ) {
                let reservation = GpuReservation {
                    id: 1,
                    user: "alice".into(),
                    gpu_spec: GpuSpec::Count(2),
                    start_time: start,
                    duration: dur,
                    status: ReservationStatus::Pending,
                    created_at: SystemTime::UNIX_EPOCH,
                    cancelled_at: None,
                };

                let end = start + dur;
                let after_start = end + Duration::from_secs(gap);
                let after_end = after_start + Duration::from_secs(100);

                prop_assert!(!reservation.overlaps_with(after_start, after_end));
            }

            /// Property: A reservation always overlaps with any time range that contains it
            #[test]
            fn prop_overlap_when_contained(
                (start, dur) in time_range_strategy(),
                before in 1u64..1000,
                after in 1u64..1000,
            ) {
                let reservation = GpuReservation {
                    id: 1,
                    user: "alice".into(),
                    gpu_spec: GpuSpec::Count(2),
                    start_time: start,
                    duration: dur,
                    status: ReservationStatus::Pending,
                    created_at: SystemTime::UNIX_EPOCH,
                    cancelled_at: None,
                };

                let end = start + dur;
                let container_start = start - Duration::from_secs(before);
                let container_end = end + Duration::from_secs(after);

                prop_assert!(reservation.overlaps_with(container_start, container_end));
            }

            /// Property: Status transitions are monotonic (never go backwards)
            /// Pending -> Active -> Completed (or Cancelled from any state)
            #[test]
            fn prop_status_monotonic(
                (start, dur) in time_range_strategy(),
            ) {
                let mut reservation = GpuReservation {
                    id: 1,
                    user: "alice".into(),
                    gpu_spec: GpuSpec::Count(2),
                    start_time: start,
                    duration: dur,
                    status: ReservationStatus::Pending,
                    created_at: SystemTime::UNIX_EPOCH,
                    cancelled_at: None,
                };

                let end = start + dur;

                // Before start: should be Pending
                let before = start - Duration::from_secs(100);
                reservation.update_status(before);
                prop_assert_eq!(reservation.status, ReservationStatus::Pending);

                // At start: should be Active
                reservation.update_status(start);
                prop_assert_eq!(reservation.status, ReservationStatus::Active);

                // After end: should be Completed
                let after = end + Duration::from_secs(100);
                reservation.update_status(after);
                prop_assert_eq!(reservation.status, ReservationStatus::Completed);

                // Once Completed, stays Completed
                reservation.update_status(after + Duration::from_secs(1000));
                prop_assert_eq!(reservation.status, ReservationStatus::Completed);
            }

            /// Property: end_time is always start_time + duration
            #[test]
            fn prop_end_time_calculation(
                (start, dur) in time_range_strategy(),
            ) {
                let reservation = GpuReservation {
                    id: 1,
                    user: "alice".into(),
                    gpu_spec: GpuSpec::Count(2),
                    start_time: start,
                    duration: dur,
                    status: ReservationStatus::Pending,
                    created_at: SystemTime::UNIX_EPOCH,
                    cancelled_at: None,
                };

                prop_assert_eq!(reservation.end_time(), start + dur);
            }

            /// Property: GpuSpec::Count always returns the count value
            #[test]
            fn prop_gpu_spec_count(count in 1u32..100) {
                let spec = GpuSpec::Count(count);
                prop_assert_eq!(spec.count(), count);
                prop_assert_eq!(spec.indices(), None);
            }

            /// Property: GpuSpec::Indices count equals the length of indices vector
            #[test]
            fn prop_gpu_spec_indices(indices in prop::collection::vec(0u32..16, 0..10)) {
                let spec = GpuSpec::Indices(indices.clone());
                prop_assert_eq!(spec.count(), indices.len() as u32);
                prop_assert_eq!(spec.indices(), Some(indices.as_slice()));
            }

            /// Property: Cancelled reservations are never active
            #[test]
            fn prop_cancelled_never_active(
                (start, dur) in time_range_strategy(),
                check_time in 1000u64..1_000_000_000,
            ) {
                let mut reservation = GpuReservation {
                    id: 1,
                    user: "alice".into(),
                    gpu_spec: GpuSpec::Count(2),
                    start_time: start,
                    duration: dur,
                    status: ReservationStatus::Cancelled,
                    created_at: SystemTime::UNIX_EPOCH,
                    cancelled_at: Some(SystemTime::UNIX_EPOCH),
                };

                let check = SystemTime::UNIX_EPOCH + Duration::from_secs(check_time);
                prop_assert!(!reservation.is_active(check));

                // Even if we try to update status, it should stay Cancelled
                reservation.update_status(check);
                prop_assert_eq!(reservation.status, ReservationStatus::Cancelled);
            }
        }
    }
}
