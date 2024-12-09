#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub command: String,                          // 要执行的命令
    pub resources_required: ResourceRequirements, // 所需资源（如 CPU、内存）
    pub status: JobStatus,                        // 任务的当前状态（等待、运行中、完成等）
}

impl Job {
    pub fn execute(&mut self, _gpu_ids: Vec<usize>) {
        // 执行任务
        self.status = JobStatus::Running;
    }
}

#[derive(Debug, Clone)]
pub struct ResourceRequirements {
    pub cpus: usize,
    pub gpus: usize,
    pub gpu_memory: usize, // in GB per GPU
}

#[derive(Debug)]
pub struct GPU {
    pub id: usize,
    pub total_memory: usize,
    pub available_memory: usize,
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
