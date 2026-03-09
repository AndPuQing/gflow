use super::*;

impl Scheduler {
    /// Update the cached state->job_ids index.
    ///
    /// This maintains sorted job IDs per state so API handlers can iterate in stable ID order
    /// without scanning all jobs.
    pub(super) fn update_state_jobs_index(
        &mut self,
        job_id: u32,
        old_state: JobState,
        new_state: JobState,
    ) {
        if old_state == new_state {
            return;
        }

        if let Some(v) = self.state_jobs_index.get_mut(&old_state) {
            if let Ok(pos) = v.binary_search(&job_id) {
                v.remove(pos);
                if v.is_empty() {
                    self.state_jobs_index.remove(&old_state);
                }
            }
        }

        let entry = self.state_jobs_index.entry(new_state).or_default();
        match entry.binary_search(&job_id) {
            Ok(_) => {} // already present
            Err(pos) => entry.insert(pos, job_id),
        }
    }

    /// Update the cached project->job_ids index.
    ///
    /// This maintains sorted job IDs per project so API handlers can iterate in stable ID order
    /// without scanning all jobs. Uses binary search to maintain sort order.
    ///
    /// Note: Projects are immutable after job submission. This method is designed for initial
    /// indexing and rebuild operations, not for updating existing job projects.
    pub(super) fn update_project_jobs_index(
        &mut self,
        job_id: u32,
        old_project: Option<&CompactString>,
        new_project: Option<&CompactString>,
    ) {
        if old_project == new_project {
            return;
        }

        // Remove from old project
        if let Some(old_proj) = old_project {
            if let Some(v) = self.project_jobs_index.get_mut(old_proj) {
                if let Ok(pos) = v.binary_search(&job_id) {
                    v.remove(pos);
                    if v.is_empty() {
                        self.project_jobs_index.remove(old_proj);
                    }
                }
            }
        }

        // Add to new project
        if let Some(new_proj) = new_project {
            let entry = self.project_jobs_index.entry(new_proj.clone()).or_default();
            match entry.binary_search(&job_id) {
                Ok(_) => {} // already present
                Err(pos) => entry.insert(pos, job_id),
            }
        }
    }

    /// Get job IDs by project for fast filtering.
    ///
    /// Returns a sorted list of job IDs (ascending order) for the given project,
    /// or None if no jobs exist for that project.
    pub fn job_ids_by_project(&self, project: &str) -> Option<&Vec<u32>> {
        self.project_jobs_index.get(project)
    }

    /// Get a JobSpec by ID (job IDs start at 1, so we subtract 1 for the index)
    #[inline]
    pub fn get_job_spec(&self, job_id: u32) -> Option<&JobSpec> {
        if job_id == 0 {
            return None;
        }
        self.job_specs.get((job_id - 1) as usize)
    }

    /// Get a JobRuntime by ID
    #[inline]
    pub fn get_job_runtime(&self, job_id: u32) -> Option<&JobRuntime> {
        if job_id == 0 {
            return None;
        }
        self.job_runtimes.get((job_id - 1) as usize)
    }

    /// Get a mutable JobRuntime by ID
    #[inline]
    pub fn get_job_runtime_mut(&mut self, job_id: u32) -> Option<&mut JobRuntime> {
        if job_id == 0 {
            return None;
        }
        self.job_runtimes.get_mut((job_id - 1) as usize)
    }

    /// Get a JobView combining spec and runtime
    pub fn get_job_view(&self, job_id: u32) -> Option<JobView> {
        let spec = self.get_job_spec(job_id)?;
        let runtime = self.get_job_runtime(job_id)?;
        Some(JobView::from_refs(spec, runtime))
    }

    /// Borrow `JobSpec + JobRuntime` for a job without allocating.
    pub fn get_job_parts(&self, job_id: u32) -> Option<(&JobSpec, &JobRuntime)> {
        let idx = job_id.checked_sub(1)? as usize;
        let spec = self.job_specs.get(idx)?;
        let rt = self.job_runtimes.get(idx)?;
        Some((spec, rt))
    }

    /// Mutably borrow `JobSpec + JobRuntime` for a job without allocating.
    pub fn get_job_parts_mut(&mut self, job_id: u32) -> Option<(&mut JobSpec, &mut JobRuntime)> {
        let idx = job_id.checked_sub(1)? as usize;
        let spec = self.job_specs.get_mut(idx)?;
        let rt = self.job_runtimes.get_mut(idx)?;
        Some((spec, rt))
    }

    /// Check invariant: job_specs and job_runtimes must have same length
    #[inline]
    pub(super) fn check_invariant(&self) {
        debug_assert_eq!(
            self.job_specs.len(),
            self.job_runtimes.len(),
            "job_specs and job_runtimes must have same length"
        );
    }

    /// Total jobs stored in the scheduler.
    #[inline]
    pub fn jobs_len(&self) -> usize {
        self.job_runtimes.len()
    }

    /// Read-only access to all job specs (cold data).
    pub fn job_specs(&self) -> &[JobSpec] {
        &self.job_specs
    }

    /// Read-only access to all job runtimes (hot data).
    pub fn job_runtimes(&self) -> &[JobRuntime] {
        &self.job_runtimes
    }

    /// Materialize a legacy `Job` by composing `JobSpec + JobRuntime`.
    ///
    /// This is intentionally **not** the primary storage representation (to keep the hot
    /// contiguous working set small). Prefer using `get_job_spec*` / `get_job_runtime*` for
    /// internal logic.
    #[inline]
    pub fn get_job(&self, job_id: u32) -> Option<Job> {
        let spec = self.get_job_spec(job_id)?;
        let runtime = self.get_job_runtime(job_id)?;
        Some(Job::from_parts(spec.clone(), runtime.clone()))
    }

    /// Materialize all jobs as legacy `Job` structs (allocates/clones).
    pub fn jobs_as_vec(&self) -> Vec<Job> {
        self.check_invariant();
        self.job_specs
            .iter()
            .zip(self.job_runtimes.iter())
            .map(|(spec, runtime)| Job::from_parts(spec.clone(), runtime.clone()))
            .collect()
    }

    /// Check if a job exists
    #[inline]
    pub fn job_exists(&self, job_id: u32) -> bool {
        job_id != 0 && (job_id as usize) <= self.job_runtimes.len()
    }

    /// Get available GPU slots respecting restrictions
    pub fn get_available_gpu_slots(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self
            .gpu_slots
            .values()
            .filter(|slot| slot.available)
            .map(|slot| slot.index)
            .filter(|&index| {
                // Apply GPU restriction filter
                match &self.allowed_gpu_indices {
                    None => true, // No restriction, all GPUs allowed
                    Some(allowed) => allowed.contains(&index),
                }
            })
            .collect();
        slots.sort_unstable();
        slots
    }

    /// Get scheduler info (GPU status and restrictions)
    pub fn info(&self) -> SchedulerInfo {
        let mut gpus: Vec<GpuInfo> = self
            .gpu_slots
            .iter()
            .map(|(uuid, slot)| GpuInfo {
                uuid: uuid.clone(),
                index: slot.index,
                available: slot.available,
                reason: slot.reason.clone(),
            })
            .collect();
        // Sort by index for stable output
        gpus.sort_by_key(|g| g.index);
        SchedulerInfo {
            gpus,
            allowed_gpu_indices: self.allowed_gpu_indices.clone(),
            gpu_allocation_strategy: self.gpu_allocation_strategy,
        }
    }

    /// Get total number of GPU slots
    pub fn gpu_slots_count(&self) -> usize {
        self.gpu_slots.len()
    }

    /// Set GPU restrictions
    pub fn set_allowed_gpu_indices(&mut self, indices: Option<Vec<u32>>) {
        self.allowed_gpu_indices = indices;
    }

    /// Get GPU restrictions
    pub fn allowed_gpu_indices(&self) -> Option<&Vec<u32>> {
        self.allowed_gpu_indices.as_ref()
    }

    /// Set GPU allocation strategy.
    pub fn set_gpu_allocation_strategy(&mut self, strategy: GpuAllocationStrategy) {
        self.gpu_allocation_strategy = strategy;
    }

    /// Get current GPU allocation strategy.
    pub fn gpu_allocation_strategy(&self) -> GpuAllocationStrategy {
        self.gpu_allocation_strategy
    }
}
