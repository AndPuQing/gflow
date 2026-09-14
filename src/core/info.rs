use serde::{Deserialize, Serialize};

use super::gpu_allocation::GpuAllocationStrategy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IgnoredGpuProcess {
    pub gpu_index: u32,
    pub pid: u32,
}

/// A compute process attached to a GPU that gflow does not manage.
///
/// gflow refuses to allocate a GPU that still has an unmanaged process on it,
/// so these entries explain *why* a card is unavailable and what the operator
/// can do about it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnmanagedGpuProcess {
    pub pid: u32,
    /// GPU memory held by the process in MB, when NVML reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_memory_mb: Option<u64>,
    /// SM (3D/compute) utilization percentage over the last NVML sample.
    /// `Some(0)` means the process holds memory but executed no kernel — the
    /// signature of an idle leftover. `None` means NVML could not report it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utilization_percent: Option<u32>,
    /// Age of the host process in seconds (Linux only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_secs: Option<u64>,
}

/// A process attached to a GPU for at least this long while doing no GPU work
/// is reported as an idle leftover rather than a real workload.
pub const UNMANAGED_IDLE_AGE_THRESHOLD_SECS: u64 = 3600;

impl UnmanagedGpuProcess {
    /// True when the process has held the GPU for a long time while running no
    /// kernels: usually a leaked/idle leftover rather than a real workload.
    pub fn is_idle_leftover(&self) -> bool {
        self.utilization_percent == Some(0)
            && self
                .age_secs
                .is_some_and(|age| age >= UNMANAGED_IDLE_AGE_THRESHOLD_SECS)
    }

    /// The command that releases this GPU without touching the process.
    pub fn release_command(&self, gpu_index: u32) -> String {
        format!(
            "gctl gpu-process ignore --gpu {} --pid {}",
            gpu_index, self.pid
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub uuid: String,
    pub index: u32,
    pub available: bool,
    /// Reason why GPU is unavailable (e.g., occupied by non-gflow process)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Non-gflow compute processes currently attached to this GPU, so callers
    /// can show per-process detail and the release command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmanaged_processes: Vec<UnmanagedGpuProcess>,
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
    /// Job executor backend: "tmux" (default) or "process".
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
    /// Job executor backend: "tmux" (default) or "process".
    #[serde(default)]
    pub executor: String,
    /// Manual runtime-only GPU process ignore overrides currently in effect.
    /// Included here so clients do not need a separate
    /// `gctl gpu-process list` call to show them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_gpu_processes: Vec<IgnoredGpuProcess>,
}
