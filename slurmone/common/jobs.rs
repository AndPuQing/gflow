#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub command: String,
    pub resources_required: ResourceRequirements,
    pub status: JobStatus,
}

impl Job {
    pub fn execute(&mut self, _gpu_ids: Vec<usize>) {
        // 执行任务
        self.status = JobStatus::Running;
    }
}

#[derive(Debug, Clone)]
pub struct ResourceRequirements {
    pub gpus: usize,
}

#[derive(Debug)]
pub struct Gpu {
    pub id: usize,
    pub is_busy: bool,
}

pub struct GPUManager {}

#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Canceled,
}
