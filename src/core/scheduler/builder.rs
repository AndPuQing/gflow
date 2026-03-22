use super::*;
use crate::core::gpu::{GPUSlot, GpuUuid};

pub struct SchedulerBuilder {
    executor: Option<Box<dyn Executor>>,
    gpu_slots: HashMap<GpuUuid, GPUSlot>,
    state_path: PathBuf,
    total_memory_mb: u64,
    allowed_gpu_indices: Option<Vec<u32>>,
    gpu_allocation_strategy: GpuAllocationStrategy,
}

impl SchedulerBuilder {
    pub fn new() -> Self {
        Self {
            executor: None,
            gpu_slots: HashMap::new(),
            state_path: PathBuf::from("state.json"),
            total_memory_mb: 16 * 1024,
            allowed_gpu_indices: None,
            gpu_allocation_strategy: GpuAllocationStrategy::default(),
        }
    }

    pub fn with_executor(mut self, executor: Box<dyn Executor>) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn with_gpu_slots(mut self, slots: HashMap<GpuUuid, GPUSlot>) -> Self {
        self.gpu_slots = slots;
        self
    }

    pub fn with_state_path(mut self, path: PathBuf) -> Self {
        self.state_path = path;
        self
    }

    pub fn with_total_memory_mb(mut self, memory_mb: u64) -> Self {
        self.total_memory_mb = memory_mb;
        self
    }

    pub fn with_allowed_gpu_indices(mut self, indices: Option<Vec<u32>>) -> Self {
        self.allowed_gpu_indices = indices;
        self
    }

    pub fn with_gpu_allocation_strategy(mut self, strategy: GpuAllocationStrategy) -> Self {
        self.gpu_allocation_strategy = strategy;
        self
    }

    pub fn build(self) -> Scheduler {
        Scheduler {
            version: crate::core::migrations::CURRENT_VERSION,
            job_specs: Vec::new(),
            job_runtimes: Vec::new(),
            executor: self.executor,
            gpu_slots: self.gpu_slots,
            total_memory_mb: self.total_memory_mb,
            available_memory_mb: self.total_memory_mb,
            state_path: self.state_path,
            next_job_id: 1,
            allowed_gpu_indices: self.allowed_gpu_indices,
            gpu_allocation_strategy: self.gpu_allocation_strategy,
            user_jobs_index: HashMap::new(),
            state_jobs_index: HashMap::new(),
            project_jobs_index: HashMap::new(),
            dependency_graph: HashMap::new(),
            dependents_graph: HashMap::new(),
            group_running_count: HashMap::new(),
            reservations: Vec::new(),
            next_reservation_id: 1,
        }
    }
}

impl Default for SchedulerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
