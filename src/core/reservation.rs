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

/// A GPU reservation for a specific user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuReservation {
    /// Unique reservation ID
    pub id: u32,
    /// Username who created the reservation
    pub user: CompactString,
    /// Number of GPUs to reserve
    pub gpu_count: u32,
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
            gpu_count: 2,
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
            gpu_count: 2,
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
            gpu_count: 2,
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
            gpu_count: 2,
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
            gpu_count: 2,
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
}
