use super::*;

impl Scheduler {
    pub fn create_reservation(
        &mut self,
        user: CompactString,
        gpu_spec: crate::core::reservation::GpuSpec,
        start_time: std::time::SystemTime,
        duration: std::time::Duration,
    ) -> anyhow::Result<u32> {
        use crate::core::conflict;
        use crate::core::reservation::{GpuReservation, ReservationStatus};

        // Validate GPU spec
        let total_gpus = self.gpu_slots_count() as u32;
        let gpu_count = gpu_spec.count();

        if gpu_count == 0 {
            anyhow::bail!("GPU count must be greater than 0");
        }
        if gpu_count > total_gpus {
            anyhow::bail!(
                "Requested {} GPUs but only {} GPUs available",
                gpu_count,
                total_gpus
            );
        }

        // Validate GPU indices if specified
        if let Some(indices) = gpu_spec.indices() {
            for &idx in indices {
                if idx >= total_gpus {
                    anyhow::bail!(
                        "GPU index {} is out of range (available: 0-{})",
                        idx,
                        total_gpus - 1
                    );
                }
            }
        }

        // Validate start time (not in past)
        let now = std::time::SystemTime::now();
        if start_time < now {
            anyhow::bail!("Start time cannot be in the past");
        }

        // Check for conflicts using pure functions
        let end_time = start_time + duration;
        let state = conflict::collect_reservation_state(&self.reservations, start_time, end_time);
        conflict::check_reservation_conflict(&gpu_spec, &state, total_gpus)?;

        // Create reservation
        let id = self.next_reservation_id;
        self.next_reservation_id += 1;

        let reservation = GpuReservation {
            id,
            user,
            gpu_spec,
            start_time,
            duration,
            status: ReservationStatus::Pending,
            created_at: now,
            cancelled_at: None,
        };

        self.reservations.push(reservation);

        // Sort reservations by start_time for efficient queries
        self.reservations.sort_by_key(|r| r.start_time);

        Ok(id)
    }

    /// Get a reservation by ID
    pub fn get_reservation(&self, id: u32) -> Option<&GpuReservation> {
        self.reservations.iter().find(|r| r.id == id)
    }

    /// Get a mutable reservation by ID
    pub fn get_reservation_mut(&mut self, id: u32) -> Option<&mut GpuReservation> {
        self.reservations.iter_mut().find(|r| r.id == id)
    }

    /// Cancel a reservation
    pub fn cancel_reservation(&mut self, id: u32) -> anyhow::Result<()> {
        use crate::core::reservation::ReservationStatus;

        let reservation = self
            .get_reservation_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Reservation {} not found", id))?;

        match reservation.status {
            ReservationStatus::Completed => {
                anyhow::bail!("Cannot cancel completed reservation");
            }
            ReservationStatus::Cancelled => {
                anyhow::bail!("Reservation already cancelled");
            }
            ReservationStatus::Pending | ReservationStatus::Active => {
                reservation.status = ReservationStatus::Cancelled;
                reservation.cancelled_at = Some(std::time::SystemTime::now());
                Ok(())
            }
        }
    }

    /// List reservations with optional filters
    pub fn list_reservations(
        &self,
        user_filter: Option<&str>,
        status_filter: Option<ReservationStatus>,
        active_only: bool,
    ) -> Vec<&GpuReservation> {
        let now = std::time::SystemTime::now();

        self.reservations
            .iter()
            .filter(|r| {
                // User filter
                if let Some(user) = user_filter {
                    if r.user != user {
                        return false;
                    }
                }

                // Status filter
                if let Some(status) = status_filter {
                    if r.status != status {
                        return false;
                    }
                }

                // Active only filter
                if active_only && !r.is_active(now) {
                    return false;
                }

                true
            })
            .collect()
    }

    /// Update reservation statuses based on current time and remove completed/cancelled ones
    pub fn update_reservation_statuses(&mut self) {
        use crate::core::reservation::ReservationStatus;

        let now = std::time::SystemTime::now();

        // Update statuses
        for reservation in &mut self.reservations {
            reservation.update_status(now);
        }

        // Remove completed/cancelled reservations immediately
        self.reservations.retain(|r| {
            matches!(
                r.status,
                ReservationStatus::Pending | ReservationStatus::Active
            )
        });
    }

    /// Get currently active reservations
    pub fn get_active_reservations(&self) -> Vec<&GpuReservation> {
        use crate::core::reservation::ReservationStatus;

        let now = std::time::SystemTime::now();

        self.reservations
            .iter()
            .filter(|r| r.status == ReservationStatus::Active && r.is_active(now))
            .collect()
    }

    /// Check if a job respects active reservations
    /// Returns true if the job can proceed, false if it should be blocked
    pub(super) fn check_job_respects_reservations(
        &self,
        job_user: &str,
        job_gpu_count: u32,
        available_gpus: &[u32],
    ) -> bool {
        use crate::core::reservation::GpuSpec;
        use std::collections::HashSet;

        let active_reservations = self.get_active_reservations();

        if active_reservations.is_empty() {
            return true; // No active reservations, job can proceed
        }

        let total_gpus = self.gpu_slots_count() as u32;

        // Collect reserved GPU indices by other users
        let mut blocked_indices = HashSet::new();
        let mut user_reserved_count = 0u32;
        let mut user_reserved_indices = Vec::new();
        let mut other_count_reserved = 0u32;

        for reservation in &active_reservations {
            if reservation.user == job_user {
                // This user's reservations
                match &reservation.gpu_spec {
                    GpuSpec::Indices(indices) => {
                        user_reserved_indices.extend(indices.iter().copied());
                    }
                    GpuSpec::Count(count) => {
                        user_reserved_count += count;
                    }
                }
            } else {
                // Other users' reservations
                match &reservation.gpu_spec {
                    GpuSpec::Indices(indices) => {
                        // Block specific GPU indices reserved by others
                        blocked_indices.extend(indices.iter().copied());
                    }
                    GpuSpec::Count(count) => {
                        // Other users' count-based reservations
                        other_count_reserved += count;
                    }
                }
            }
        }

        // If user has index-based reservation, they can use those specific GPUs
        if !user_reserved_indices.is_empty() {
            return job_gpu_count <= user_reserved_indices.len() as u32;
        }

        // If user has count-based reservation, they can use unreserved GPUs
        if user_reserved_count > 0 {
            return job_gpu_count <= user_reserved_count;
        }

        // User has no reservation - can only use GPUs not blocked by index-based reservations
        // and not needed by other count-based reservations
        let available_for_unreserved = total_gpus
            .saturating_sub(blocked_indices.len() as u32)
            .saturating_sub(other_count_reserved);

        // Check that job doesn't exceed available unreserved GPUs
        // and that there are enough physically available GPUs
        let usable_gpus: Vec<u32> = available_gpus
            .iter()
            .filter(|&&gpu| !blocked_indices.contains(&gpu))
            .copied()
            .collect();

        job_gpu_count <= available_for_unreserved && job_gpu_count <= usable_gpus.len() as u32
    }

    /// Filter available GPUs to only include those usable by the given user
    /// considering active reservations
    pub(super) fn filter_usable_gpus(&self, job_user: &str, available_gpus: &[u32]) -> Vec<u32> {
        use crate::core::reservation::GpuSpec;
        use std::collections::HashSet;

        let active_reservations = self.get_active_reservations();

        if active_reservations.is_empty() {
            return available_gpus.to_vec();
        }

        // Collect reserved GPU indices and user's reservations
        let mut blocked_indices = HashSet::new();
        let mut user_reserved_indices = Vec::new();

        for reservation in &active_reservations {
            if reservation.user == job_user {
                // This user's index-based reservations
                if let GpuSpec::Indices(indices) = &reservation.gpu_spec {
                    user_reserved_indices.extend(indices.iter().copied());
                }
            } else {
                // Other users' index-based reservations block these GPUs
                if let GpuSpec::Indices(indices) = &reservation.gpu_spec {
                    blocked_indices.extend(indices.iter().copied());
                }
            }
        }

        // If user has index-based reservation, prioritize those GPUs
        if !user_reserved_indices.is_empty() {
            return user_reserved_indices
                .into_iter()
                .filter(|gpu| available_gpus.contains(gpu))
                .collect();
        }

        // Otherwise, use any GPU not blocked by others
        available_gpus
            .iter()
            .filter(|&&gpu| !blocked_indices.contains(&gpu))
            .copied()
            .collect()
    }

    /// Reorder candidate GPU indices according to configured allocation strategy.
    pub(super) fn reorder_usable_gpus(&self, job_id: u32, usable_gpus: &mut [u32]) {
        match self.gpu_allocation_strategy {
            GpuAllocationStrategy::Sequential => {
                usable_gpus.sort_unstable();
            }
            GpuAllocationStrategy::Random => {
                let time_seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() ^ ((d.subsec_nanos() as u64) << 32))
                    .unwrap_or(0);
                let seed = time_seed ^ ((job_id as u64) << 32) ^ (self.next_job_id as u64);

                usable_gpus.sort_unstable_by_key(|gpu| {
                    splitmix64(seed ^ ((*gpu as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)))
                });
            }
        }
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}
