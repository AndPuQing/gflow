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
}
