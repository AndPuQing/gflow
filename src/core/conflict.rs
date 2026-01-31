//! Pure functions for GPU reservation conflict detection
//!
//! This module contains pure, side-effect-free functions for detecting conflicts
//! between GPU reservations. These functions are easier to test, reason about,
//! and formally verify than stateful methods.

use crate::core::reservation::{GpuReservation, GpuSpec, ReservationStatus};
use std::collections::HashSet;
use std::time::SystemTime;

/// Error type for reservation conflicts
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictError {
    /// GPU index is already reserved by another reservation
    IndexConflict { index: u32 },
    /// Not enough unreserved GPUs available for count-based reservations
    InsufficientGpusForCount {
        available: u32,
        required: u32,
        reserved_indices: usize,
        count_based_reserved: u32,
    },
    /// Would leave insufficient GPUs for existing count-based reservations
    WouldStarveCountBased {
        available_after: u32,
        count_based_reserved: u32,
    },
}

impl std::fmt::Display for ConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictError::IndexConflict { index } => {
                write!(
                    f,
                    "GPU index {} is already reserved during this time period",
                    index
                )
            }
            ConflictError::InsufficientGpusForCount {
                available,
                required,
                reserved_indices,
                count_based_reserved,
            } => {
                write!(
                    f,
                    "Reservation conflicts: {} GPUs explicitly reserved, {} GPUs reserved by count, cannot reserve {} more (available: {})",
                    reserved_indices, count_based_reserved, required, available
                )
            }
            ConflictError::WouldStarveCountBased {
                available_after,
                count_based_reserved,
            } => {
                write!(
                    f,
                    "Cannot reserve GPU indices: would leave insufficient GPUs ({}) for existing count-based reservations ({})",
                    available_after, count_based_reserved
                )
            }
        }
    }
}

impl std::error::Error for ConflictError {}

/// Represents the state of GPU reservations at a specific time range
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationState {
    /// Set of GPU indices that are explicitly reserved
    pub reserved_indices: HashSet<u32>,
    /// Total number of GPUs reserved by count-based reservations
    pub count_based_reserved: u32,
}

impl ReservationState {
    /// Create an empty reservation state
    pub fn empty() -> Self {
        Self {
            reserved_indices: HashSet::new(),
            count_based_reserved: 0,
        }
    }

    /// Calculate available GPUs for count-based reservations
    pub fn available_for_count(&self, total_gpus: u32) -> u32 {
        total_gpus.saturating_sub(self.reserved_indices.len() as u32)
    }
}

/// Collect the reservation state for a given time range
///
/// This is a pure function that analyzes existing reservations and returns
/// the state of GPU allocations during the specified time range.
///
/// # Arguments
/// * `reservations` - Slice of existing reservations to analyze
/// * `start_time` - Start of the time range to check
/// * `end_time` - End of the time range to check
///
/// # Returns
/// A `ReservationState` containing the reserved indices and count-based reservations
pub fn collect_reservation_state(
    reservations: &[GpuReservation],
    start_time: SystemTime,
    end_time: SystemTime,
) -> ReservationState {
    let mut state = ReservationState::empty();

    for reservation in reservations {
        // Skip cancelled reservations
        if reservation.status == ReservationStatus::Cancelled {
            continue;
        }

        // Check if this reservation overlaps with the requested time range
        if reservation.overlaps_with(start_time, end_time) {
            match &reservation.gpu_spec {
                GpuSpec::Indices(indices) => {
                    state.reserved_indices.extend(indices.iter().copied());
                }
                GpuSpec::Count(count) => {
                    state.count_based_reserved += count;
                }
            }
        }
    }

    state
}

/// Check if an index-based reservation conflicts with existing reservations
///
/// This is a pure function that checks whether reserving specific GPU indices
/// would conflict with existing reservations.
///
/// # Arguments
/// * `new_indices` - The GPU indices to reserve
/// * `state` - Current reservation state
/// * `total_gpus` - Total number of GPUs in the system
///
/// # Returns
/// `Ok(())` if no conflict, `Err(ConflictError)` if there is a conflict
pub fn check_index_reservation_conflict(
    new_indices: &[u32],
    state: &ReservationState,
    total_gpus: u32,
) -> Result<(), ConflictError> {
    // Check if any requested index is already reserved
    for &idx in new_indices {
        if state.reserved_indices.contains(&idx) {
            return Err(ConflictError::IndexConflict { index: idx });
        }
    }

    // Check if there are enough unreserved GPUs for existing count-based reservations
    let available_for_count = total_gpus
        .saturating_sub(state.reserved_indices.len() as u32)
        .saturating_sub(new_indices.len() as u32);

    if state.count_based_reserved > available_for_count {
        return Err(ConflictError::WouldStarveCountBased {
            available_after: available_for_count,
            count_based_reserved: state.count_based_reserved,
        });
    }

    Ok(())
}

/// Check if a count-based reservation conflicts with existing reservations
///
/// This is a pure function that checks whether reserving a count of GPUs
/// would conflict with existing reservations.
///
/// # Arguments
/// * `new_count` - Number of GPUs to reserve
/// * `state` - Current reservation state
/// * `total_gpus` - Total number of GPUs in the system
///
/// # Returns
/// `Ok(())` if no conflict, `Err(ConflictError)` if there is a conflict
pub fn check_count_reservation_conflict(
    new_count: u32,
    state: &ReservationState,
    total_gpus: u32,
) -> Result<(), ConflictError> {
    let available_for_count = state.available_for_count(total_gpus);

    if state.count_based_reserved + new_count > available_for_count {
        return Err(ConflictError::InsufficientGpusForCount {
            available: available_for_count,
            required: new_count,
            reserved_indices: state.reserved_indices.len(),
            count_based_reserved: state.count_based_reserved,
        });
    }

    Ok(())
}

/// Check if a reservation conflicts with existing reservations
///
/// This is the main pure function that checks for conflicts. It delegates
/// to the appropriate specialized function based on the GPU spec type.
///
/// # Arguments
/// * `gpu_spec` - The GPU specification to check
/// * `state` - Current reservation state
/// * `total_gpus` - Total number of GPUs in the system
///
/// # Returns
/// `Ok(())` if no conflict, `Err(ConflictError)` if there is a conflict
pub fn check_reservation_conflict(
    gpu_spec: &GpuSpec,
    state: &ReservationState,
    total_gpus: u32,
) -> Result<(), ConflictError> {
    match gpu_spec {
        GpuSpec::Indices(indices) => check_index_reservation_conflict(indices, state, total_gpus),
        GpuSpec::Count(count) => check_count_reservation_conflict(*count, state, total_gpus),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_reservation(
        id: u32,
        gpu_spec: GpuSpec,
        start_secs: u64,
        duration_secs: u64,
    ) -> GpuReservation {
        GpuReservation {
            id,
            user: format!("user{}", id).into(),
            gpu_spec,
            start_time: SystemTime::UNIX_EPOCH + Duration::from_secs(start_secs),
            duration: Duration::from_secs(duration_secs),
            status: ReservationStatus::Pending,
            created_at: SystemTime::UNIX_EPOCH,
            cancelled_at: None,
        }
    }

    #[test]
    fn test_empty_state() {
        let state = ReservationState::empty();
        assert_eq!(state.reserved_indices.len(), 0);
        assert_eq!(state.count_based_reserved, 0);
        assert_eq!(state.available_for_count(8), 8);
    }

    #[test]
    fn test_collect_reservation_state_no_overlap() {
        let reservations = vec![
            create_test_reservation(1, GpuSpec::Indices(vec![0, 1]), 1000, 3600),
            create_test_reservation(2, GpuSpec::Count(2), 5000, 3600),
        ];

        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(10000);
        let end = start + Duration::from_secs(3600);

        let state = collect_reservation_state(&reservations, start, end);
        assert_eq!(state.reserved_indices.len(), 0);
        assert_eq!(state.count_based_reserved, 0);
    }

    #[test]
    fn test_collect_reservation_state_with_overlap() {
        let reservations = vec![
            create_test_reservation(1, GpuSpec::Indices(vec![0, 1]), 1000, 3600),
            create_test_reservation(2, GpuSpec::Count(2), 1000, 3600),
        ];

        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);
        let end = start + Duration::from_secs(1000);

        let state = collect_reservation_state(&reservations, start, end);
        assert_eq!(state.reserved_indices.len(), 2);
        assert!(state.reserved_indices.contains(&0));
        assert!(state.reserved_indices.contains(&1));
        assert_eq!(state.count_based_reserved, 2);
    }

    #[test]
    fn test_collect_reservation_state_skips_cancelled() {
        let mut reservations = vec![
            create_test_reservation(1, GpuSpec::Indices(vec![0, 1]), 1000, 3600),
            create_test_reservation(2, GpuSpec::Count(2), 1000, 3600),
        ];
        reservations[0].status = ReservationStatus::Cancelled;

        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);
        let end = start + Duration::from_secs(1000);

        let state = collect_reservation_state(&reservations, start, end);
        assert_eq!(state.reserved_indices.len(), 0);
        assert_eq!(state.count_based_reserved, 2);
    }

    #[test]
    fn test_check_index_conflict_no_conflict() {
        let mut state = ReservationState::empty();
        state.reserved_indices.insert(0);
        state.reserved_indices.insert(1);

        let result = check_index_reservation_conflict(&[2, 3], &state, 8);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_index_conflict_with_conflict() {
        let mut state = ReservationState::empty();
        state.reserved_indices.insert(0);
        state.reserved_indices.insert(1);

        let result = check_index_reservation_conflict(&[1, 2], &state, 8);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            ConflictError::IndexConflict { index: 1 }
        );
    }

    #[test]
    fn test_check_index_would_starve_count_based() {
        let mut state = ReservationState::empty();
        state.reserved_indices.insert(0);
        state.reserved_indices.insert(1);
        state.count_based_reserved = 4;

        // Total 8 GPUs, 2 reserved by indices, trying to reserve 2 more
        // Would leave 4 GPUs for count-based, but 4 are already reserved
        let result = check_index_reservation_conflict(&[2, 3], &state, 8);
        assert!(result.is_ok()); // Should be OK: 8 - 2 - 2 = 4, exactly enough

        // But if we try to reserve 3 more, it should fail
        let result = check_index_reservation_conflict(&[2, 3, 4], &state, 8);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_count_conflict_no_conflict() {
        let mut state = ReservationState::empty();
        state.reserved_indices.insert(0);
        state.reserved_indices.insert(1);
        state.count_based_reserved = 2;

        // Total 8 GPUs, 2 reserved by indices, 2 by count, trying to reserve 4 more
        // Available: 8 - 2 = 6, used: 2, trying: 4, total: 6 - OK
        let result = check_count_reservation_conflict(4, &state, 8);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_count_conflict_with_conflict() {
        let mut state = ReservationState::empty();
        state.reserved_indices.insert(0);
        state.reserved_indices.insert(1);
        state.count_based_reserved = 4;

        // Total 8 GPUs, 2 reserved by indices, 4 by count, trying to reserve 3 more
        // Available: 8 - 2 = 6, used: 4, trying: 3, total: 7 > 6 - CONFLICT
        let result = check_count_reservation_conflict(3, &state, 8);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_reservation_conflict_indices() {
        let mut state = ReservationState::empty();
        state.reserved_indices.insert(0);

        let result = check_reservation_conflict(&GpuSpec::Indices(vec![1, 2]), &state, 8);
        assert!(result.is_ok());

        let result = check_reservation_conflict(&GpuSpec::Indices(vec![0, 1]), &state, 8);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_reservation_conflict_count() {
        let mut state = ReservationState::empty();
        state.count_based_reserved = 4;

        let result = check_reservation_conflict(&GpuSpec::Count(4), &state, 8);
        assert!(result.is_ok());

        let result = check_reservation_conflict(&GpuSpec::Count(5), &state, 8);
        assert!(result.is_err());
    }

    // Property-based tests for conflict detection
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Property: Empty state never conflicts
            #[test]
            fn prop_empty_state_never_conflicts(
                total_gpus in 2u32..16,
                gpu_count in 1u32..8,
            ) {
                let gpu_count = std::cmp::min(gpu_count, total_gpus);
                let state = ReservationState::empty();

                let result = check_count_reservation_conflict(gpu_count, &state, total_gpus);
                prop_assert!(result.is_ok());
            }

            /// Property: Reserving all GPUs with indices should succeed if none reserved
            #[test]
            fn prop_reserve_all_indices_when_empty(
                total_gpus in 2u32..8,
            ) {
                let state = ReservationState::empty();
                let all_indices: Vec<u32> = (0..total_gpus).collect();

                let result = check_index_reservation_conflict(&all_indices, &state, total_gpus);
                prop_assert!(result.is_ok());
            }

            /// Property: Reserving an already reserved index always fails
            #[test]
            fn prop_reserved_index_always_conflicts(
                total_gpus in 4u32..16,
                reserved_idx in 0u32..4,
            ) {
                let mut state = ReservationState::empty();
                state.reserved_indices.insert(reserved_idx);

                let result = check_index_reservation_conflict(&[reserved_idx], &state, total_gpus);
                prop_assert!(result.is_err());
                if let Err(ConflictError::IndexConflict { index }) = result {
                    prop_assert_eq!(index, reserved_idx);
                }
            }

            /// Property: Total count-based reservations cannot exceed available GPUs
            #[test]
            fn prop_count_cannot_exceed_available(
                total_gpus in 2u32..16,
                reserved_count in 0u32..8,
                new_count in 1u32..8,
            ) {
                let reserved_count = std::cmp::min(reserved_count, total_gpus);
                let mut state = ReservationState::empty();
                state.count_based_reserved = reserved_count;

                let result = check_count_reservation_conflict(new_count, &state, total_gpus);

                if reserved_count + new_count <= total_gpus {
                    prop_assert!(result.is_ok());
                } else {
                    prop_assert!(result.is_err());
                }
            }

            /// Property: Index reservations respect count-based reservations
            #[test]
            fn prop_indices_respect_count_based(
                total_gpus in 4u32..16,
                count_based in 1u32..8,
                num_indices in 1usize..4,
            ) {
                let count_based = std::cmp::min(count_based, total_gpus - 1);
                let mut state = ReservationState::empty();
                state.count_based_reserved = count_based;

                let indices: Vec<u32> = (0..std::cmp::min(num_indices as u32, total_gpus)).collect();
                let result = check_index_reservation_conflict(&indices, &state, total_gpus);

                let available_after = total_gpus.saturating_sub(indices.len() as u32);
                if count_based <= available_after {
                    prop_assert!(result.is_ok());
                } else {
                    prop_assert!(result.is_err());
                    if let Err(ConflictError::WouldStarveCountBased { available_after: avail, count_based_reserved }) = result {
                        prop_assert_eq!(count_based_reserved, count_based);
                        prop_assert_eq!(avail, available_after);
                    }
                }
            }

            /// Property: Disjoint index sets never conflict
            #[test]
            fn prop_disjoint_indices_no_conflict(
                total_gpus in 8u32..16,
            ) {
                let mut state = ReservationState::empty();
                state.reserved_indices.insert(0);
                state.reserved_indices.insert(1);

                // Reserve indices that don't overlap
                let new_indices = vec![4, 5, 6];
                let result = check_index_reservation_conflict(&new_indices, &state, total_gpus);
                prop_assert!(result.is_ok());
            }

            /// Property: Overlapping index sets always conflict
            #[test]
            fn prop_overlapping_indices_conflict(
                total_gpus in 4u32..16,
                overlap_idx in 0u32..4,
            ) {
                let mut state = ReservationState::empty();
                state.reserved_indices.insert(overlap_idx);

                let new_indices = vec![overlap_idx, overlap_idx + 1];
                let result = check_index_reservation_conflict(&new_indices, &state, total_gpus);
                prop_assert!(result.is_err());
            }

            /// Property: collect_reservation_state is idempotent
            #[test]
            fn prop_collect_state_idempotent(
                start_secs in 1000u64..10000,
                duration_secs in 1000u64..5000,
            ) {
                let reservations = vec![
                    create_test_reservation(1, GpuSpec::Indices(vec![0, 1]), 2000, 3600),
                    create_test_reservation(2, GpuSpec::Count(2), 2000, 3600),
                ];

                let start = SystemTime::UNIX_EPOCH + Duration::from_secs(start_secs);
                let end = start + Duration::from_secs(duration_secs);

                let state1 = collect_reservation_state(&reservations, start, end);
                let state2 = collect_reservation_state(&reservations, start, end);

                prop_assert_eq!(state1, state2);
            }

            /// Property: Cancelled reservations are ignored
            #[test]
            fn prop_cancelled_ignored(
                start_secs in 2000u64..5000,
            ) {
                let mut reservations = vec![
                    create_test_reservation(1, GpuSpec::Indices(vec![0, 1]), 1000, 10000),
                ];
                reservations[0].status = ReservationStatus::Cancelled;

                let start = SystemTime::UNIX_EPOCH + Duration::from_secs(start_secs);
                let end = start + Duration::from_secs(1000);

                let state = collect_reservation_state(&reservations, start, end);
                prop_assert_eq!(state.reserved_indices.len(), 0);
                prop_assert_eq!(state.count_based_reserved, 0);
            }

            /// Property: available_for_count is always <= total_gpus
            #[test]
            fn prop_available_bounded(
                total_gpus in 2u32..16,
                num_reserved in 0usize..8,
            ) {
                let mut state = ReservationState::empty();
                for i in 0..std::cmp::min(num_reserved, total_gpus as usize) {
                    state.reserved_indices.insert(i as u32);
                }

                let available = state.available_for_count(total_gpus);
                prop_assert!(available <= total_gpus);
                prop_assert_eq!(available, total_gpus.saturating_sub(state.reserved_indices.len() as u32));
            }

            /// Property: Conflict check is consistent with manual calculation
            #[test]
            fn prop_conflict_check_consistent(
                total_gpus in 4u32..16,
                reserved_indices_count in 0usize..4,
                count_based in 0u32..4,
                new_count in 1u32..4,
            ) {
                let mut state = ReservationState::empty();
                for i in 0..std::cmp::min(reserved_indices_count, total_gpus as usize) {
                    state.reserved_indices.insert(i as u32);
                }
                state.count_based_reserved = std::cmp::min(count_based, total_gpus);

                let result = check_count_reservation_conflict(new_count, &state, total_gpus);

                let available = total_gpus.saturating_sub(state.reserved_indices.len() as u32);
                let would_exceed = state.count_based_reserved + new_count > available;

                if would_exceed {
                    prop_assert!(result.is_err());
                } else {
                    prop_assert!(result.is_ok());
                }
            }
        }
    }
}
