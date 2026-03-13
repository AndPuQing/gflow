#[derive(Debug, Clone)]
pub struct GPUSlot {
    pub index: u32,
    pub available: bool,
    /// Total GPU memory in MB, if known from NVML.
    pub total_memory_mb: Option<u64>,
    /// Reason why GPU is unavailable (e.g., occupied by non-gflow process)
    pub reason: Option<String>,
}

pub type GpuUuid = String;
