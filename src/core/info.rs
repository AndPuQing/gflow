use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub uuid: String,
    pub index: u32,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerInfo {
    pub gpus: Vec<GpuInfo>,
    /// GPU indices that scheduler is configured to use (None = all GPUs)
    pub allowed_gpu_indices: Option<Vec<u32>>,
}
