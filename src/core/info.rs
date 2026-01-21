use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub uuid: String,
    pub index: u32,
    pub available: bool,
    /// Reason why GPU is unavailable (e.g., occupied by non-gflow process)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerInfo {
    pub gpus: Vec<GpuInfo>,
    /// GPU indices that scheduler is configured to use (None = all GPUs)
    pub allowed_gpu_indices: Option<Vec<u32>>,
}
