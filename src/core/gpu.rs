#[derive(Debug, Clone)]
pub struct GPUSlot {
    pub index: u32,
    pub available: bool,
    /// Total GPU memory in MB, if known from NVML.
    pub total_memory_mb: Option<u64>,
    /// Total device memory in MB currently in use, as reported by NVML. This
    /// includes memory held by processes the scheduler does not manage (e.g. a
    /// user-started vLLM server), so it is the ground truth for whether a new
    /// job's declared requirement can physically fit on the device.
    /// `None` when NVML cannot report it (e.g. WDDM).
    pub used_memory_mb: Option<u64>,
    /// Reason why GPU is unavailable (e.g., occupied by non-gflow process)
    pub reason: Option<String>,
}

pub type GpuUuid = String;
