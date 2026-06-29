mod event_loop;
mod gpu;
mod jobs;
mod monitors;
mod persistence;
mod retry;
mod serialization;
#[cfg(test)]
mod tests;

pub use event_loop::run_event_driven;

use super::state_saver::StateSaverHandle;
use anyhow::{bail, Context, Result};
use compact_str::CompactString;
use gflow::core::executor::Executor;
use gflow::core::gpu::{GPUSlot, GpuUuid};
use gflow::core::info::IgnoredGpuProcess;
use gflow::core::job::{GpuSharingMode, Job, JobSpec, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::tmux::disable_pipe_pane_for_job;
use nvml_wrapper::Nvml;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;

pub type SharedState = Arc<RwLock<SchedulerRuntime>>;

/// Wrapper to make Arc<dyn Executor> compatible with Box<dyn Executor>
struct ArcExecutorWrapper(Arc<dyn Executor>);

impl Executor for ArcExecutorWrapper {
    fn execute(&self, job: &Job) -> Result<()> {
        self.0.execute(job)
    }
}

/// Runtime adapter for Scheduler with system integration
pub struct SchedulerRuntime {
    scheduler: Scheduler,
    projects_config: gflow::config::ProjectsConfig,
    nvml: Option<Nvml>,
    executor: Arc<dyn Executor>, // Shared executor for lock-free job execution
    dirty: bool,                 // Tracks if state has changed since last save
    state_saver: Option<StateSaverHandle>, // Handle for async background state persistence
    state_writable: bool,        // False when state load/migration failed
    state_load_error: Option<String>,
    state_backup_path: Option<PathBuf>,
    journal_path: PathBuf,
    journal_writable: bool,
    journal_error: Option<String>,
    journal_applied: bool,
    ignored_gpu_processes: HashSet<IgnoredGpuProcess>,
}

impl SchedulerRuntime {
    /// Create a new scheduler runtime with state loading and NVML initialization
    pub fn with_state_path(
        executor: Box<dyn Executor>,
        state_dir: PathBuf,
        allowed_gpu_indices: Option<Vec<u32>>,
        gpu_allocation_strategy: gflow::core::gpu_allocation::GpuAllocationStrategy,
        projects_config: gflow::config::ProjectsConfig,
    ) -> anyhow::Result<Self> {
        // Try to initialize NVML, but continue without it if it fails
        let (nvml, gpu_slots) = match Nvml::init() {
            Ok(nvml) => {
                let gpu_slots = Self::get_gpus(&nvml);
                (Some(nvml), gpu_slots)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize NVML: {}. Running without GPU support.",
                    e
                );
                if is_apple_silicon() {
                    tracing::info!("Apple Silicon detected; creating synthetic GPU slot with unified memory.");
                    let mut slots = HashMap::new();
                    slots.insert(
                        "apple-gpu-0".to_string(),
                        GPUSlot {
                            index: 0,
                            available: true,
                            total_memory_mb: None,
                            reason: None,
                        },
                    );
                    (None, slots)
                } else {
                    (None, HashMap::new())
                }
            }
        };

        // Validate and filter allowed GPU indices
        let validated_gpu_indices = if let Some(ref allowed) = allowed_gpu_indices {
            let detected_count = gpu_slots.len();
            let (valid, invalid): (Vec<_>, Vec<_>) = allowed
                .iter()
                .copied()
                .partition(|&idx| idx < detected_count as u32);

            if !invalid.is_empty() {
                tracing::warn!(
                    "Invalid GPU indices {:?} specified (only {} GPUs detected). These will be filtered out.",
                    invalid,
                    detected_count
                );
            }

            if valid.is_empty() {
                tracing::warn!(
                    "No valid GPU indices remaining after filtering. Allowing all GPUs."
                );
                None
            } else {
                tracing::info!("GPU restriction enabled: allowing only GPUs {:?}", valid);
                Some(valid)
            }
        } else {
            None
        };

        let total_memory_mb = Self::get_total_system_memory_mb();
        let unified_memory = is_apple_silicon() && nvml.is_none();

        // Store executor in Arc for lock-free access during job execution
        let executor_arc: Arc<dyn Executor> = Arc::from(executor);

        // Clone Arc for scheduler
        let executor_for_scheduler: Box<dyn Executor> =
            Box::new(ArcExecutorWrapper(executor_arc.clone()));

        let state_file = state_dir.join("state.json");
        let journal_path = state_dir.join("state.journal.jsonl");
        let scheduler = SchedulerBuilder::new()
            .with_executor(executor_for_scheduler)
            .with_gpu_slots(gpu_slots)
            .with_state_path(state_file)
            .with_total_memory_mb(total_memory_mb)
            .with_allowed_gpu_indices(validated_gpu_indices)
            .with_gpu_allocation_strategy(gpu_allocation_strategy)
            .with_unified_memory(unified_memory)
            .build();

        let mut runtime = Self {
            scheduler,
            projects_config,
            nvml,
            executor: executor_arc,
            dirty: false,
            state_saver: None,
            state_writable: true,
            state_load_error: None,
            state_backup_path: None,
            journal_path,
            journal_writable: false,
            journal_error: None,
            journal_applied: false,
            ignored_gpu_processes: HashSet::new(),
        };
        runtime.load_state();
        runtime.init_journal();
        Ok(runtime)
    }

    pub fn state_writable(&self) -> bool {
        self.state_writable
    }

    pub fn journal_writable(&self) -> bool {
        self.journal_writable
    }

    pub fn persistence_mode(&self) -> &'static str {
        if self.state_writable {
            "state"
        } else if self.journal_writable {
            "journal"
        } else {
            "read_only"
        }
    }

    pub fn can_mutate(&self) -> bool {
        self.state_writable || self.journal_writable
    }

    pub fn state_load_error(&self) -> Option<&str> {
        self.state_load_error.as_deref()
    }

    pub fn state_backup_path(&self) -> Option<&std::path::Path> {
        self.state_backup_path.as_deref()
    }

    pub fn journal_path(&self) -> &std::path::Path {
        &self.journal_path
    }

    pub fn journal_error(&self) -> Option<&str> {
        self.journal_error.as_deref()
    }

    /// Get total system memory in MB by reading /proc/meminfo (Linux)
    fn get_total_system_memory_mb() -> u64 {
        // Try to read /proc/meminfo on Linux
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    // MemTotal:       32864256 kB
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb / 1024; // Convert KB to MB
                        }
                    }
                }
            }
        }

        // macOS: use sysctl to read hw.memsize (bytes)
        if cfg!(target_os = "macos") {
            if let Ok(output) =
                std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output()
            {
                if let Ok(s) = std::str::from_utf8(&output.stdout) {
                    if let Ok(bytes) = s.trim().parse::<u64>() {
                        return bytes / (1024 * 1024);
                    }
                }
            }
        }

        // Fallback: assume 16GB if we can't read system memory
        tracing::warn!("Could not read system memory, assuming 16GB");
        16 * 1024
    }

    // Read-only delegated methods (no state changes)

    pub fn resolve_dependency(&self, username: &str, shorthand: &str) -> Option<u32> {
        self.scheduler.resolve_dependency(username, shorthand)
    }

    pub fn info(&self) -> gflow::core::info::SchedulerInfo {
        self.scheduler.info()
    }

    pub fn gpu_slots_count(&self) -> usize {
        self.scheduler.gpu_slots_count()
    }

    pub fn set_allowed_gpu_indices(&mut self, indices: Option<Vec<u32>>) {
        self.scheduler.set_allowed_gpu_indices(indices);
        self.mark_dirty();
    }

    pub fn gpu_available(&self, gpu_index: u32) -> Option<bool> {
        self.scheduler
            .info()
            .gpus
            .into_iter()
            .find(|gpu| gpu.index == gpu_index)
            .map(|gpu| gpu.available)
    }

    // Materialize all jobs for server handlers (allocates/clones).
    pub fn jobs(&self) -> Vec<Job> {
        self.scheduler.jobs_as_vec()
    }

    // Get a job by ID (materialized).
    pub fn get_job(&self, job_id: u32) -> Option<Job> {
        self.scheduler.get_job(job_id)
    }

    // Read-only access to hot runtimes for monitors/metrics.
    pub fn job_runtimes(&self) -> &[gflow::core::job::JobRuntime] {
        self.scheduler.job_runtimes()
    }

    // Read-only access to cold specs (used by list APIs to avoid full materialization).
    pub fn job_specs(&self) -> &[JobSpec] {
        self.scheduler.job_specs()
    }

    pub fn job_ids_by_user(&self, username: &str) -> Option<&[u32]> {
        self.scheduler.job_ids_by_user(username)
    }

    pub fn job_ids_by_state(&self, state: gflow::core::job::JobState) -> Option<&[u32]> {
        self.scheduler.job_ids_by_state(state)
    }

    // Debug/metrics accessors
    pub fn next_job_id(&self) -> u32 {
        self.scheduler.next_job_id()
    }

    pub fn validate_no_circular_dependency(
        &self,
        new_job_id: u32,
        dependency_ids: &[u32],
    ) -> Result<(), String> {
        self.scheduler
            .validate_no_circular_dependency(new_job_id, dependency_ids)
    }

    pub fn total_memory_mb(&self) -> u64 {
        self.scheduler.total_memory_mb()
    }

    pub fn available_memory_mb(&self) -> u64 {
        self.scheduler.available_memory_mb()
    }

    // GPU Reservation methods
    pub fn create_reservation(
        &mut self,
        user: compact_str::CompactString,
        gpu_spec: gflow::core::reservation::GpuSpec,
        start_time: std::time::SystemTime,
        duration: std::time::Duration,
    ) -> anyhow::Result<u32> {
        let result = self
            .scheduler
            .create_reservation(user, gpu_spec, start_time, duration)?;
        self.mark_dirty();
        Ok(result)
    }

    pub fn get_reservation(&self, id: u32) -> Option<&gflow::core::reservation::GpuReservation> {
        self.scheduler.get_reservation(id)
    }

    pub fn cancel_reservation(&mut self, id: u32) -> anyhow::Result<()> {
        self.scheduler.cancel_reservation(id)?;
        self.mark_dirty();
        Ok(())
    }

    pub fn list_reservations(
        &self,
        user_filter: Option<&str>,
        status_filter: Option<gflow::core::reservation::ReservationStatus>,
        active_only: bool,
    ) -> Vec<&gflow::core::reservation::GpuReservation> {
        self.scheduler
            .list_reservations(user_filter, status_filter, active_only)
    }

    fn get_gpus(nvml: &Nvml) -> HashMap<GpuUuid, GPUSlot> {
        let mut gpu_slots = HashMap::new();
        let device_count = nvml.device_count().unwrap_or(0);
        for i in 0..device_count {
            if let Ok(device) = nvml.device_by_index(i) {
                if let Ok(uuid) = device.uuid() {
                    let total_memory_mb = device
                        .memory_info()
                        .ok()
                        .map(|mi| mi.total / (1024_u64 * 1024_u64));
                    gpu_slots.insert(
                        uuid,
                        GPUSlot {
                            available: true,
                            index: i,
                            total_memory_mb,
                            reason: None,
                        },
                    );
                }
            }
        }
        gpu_slots
    }
}

/// Returns true when running on Apple Silicon (macOS + aarch64).
fn is_apple_silicon() -> bool {
    cfg!(target_os = "macos") && std::env::consts::ARCH == "aarch64"
}
