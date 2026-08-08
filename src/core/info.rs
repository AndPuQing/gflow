use serde::{Deserialize, Serialize};

use super::gpu_allocation::GpuAllocationStrategy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IgnoredGpuProcess {
    pub gpu_index: u32,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub uuid: String,
    pub index: u32,
    pub available: bool,
    /// Reason why GPU is unavailable (e.g., occupied by non-gflow process)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Rich daemon status payload exposed by `GET /status` for `gflowd status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// gflow version (first line of `gflowd --version` output).
    pub version: String,
    /// Daemon process ID.
    pub pid: u32,
    /// Seconds since the daemon started.
    pub uptime_secs: u64,
    /// Job executor backend: "process" (default) or "tmux".
    pub executor: String,
    /// Number of detected GPU slots.
    pub gpu_total: usize,
    /// Number of GPU slots currently available.
    pub gpu_available: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerInfo {
    pub gpus: Vec<GpuInfo>,
    /// GPU indices that scheduler is configured to use (None = all GPUs)
    pub allowed_gpu_indices: Option<Vec<u32>>,
    /// Strategy used when allocating GPUs for new jobs.
    pub gpu_allocation_strategy: GpuAllocationStrategy,
    /// Job executor backend: "process" (default) or "tmux".
    #[serde(default)]
    pub executor: String,
}
