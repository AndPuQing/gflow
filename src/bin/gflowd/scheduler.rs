use crate::executor::TmuxExecutor;
use gflow::core::executor::Executor;
use gflow::core::get_data_dir;
use gflow::core::{
    job::{Job, JobState},
    GPUSlot, GPU, UUID,
};
use nvml_wrapper::Nvml;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;

pub type SharedState = Arc<RwLock<Scheduler>>;

use gflow::core::info::{GpuInfo, SchedulerInfo};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Scheduler {
    pub jobs: HashMap<u32, Job>,
    #[serde(skip)]
    gpu_slots: HashMap<UUID, GPUSlot>,
    #[serde(skip)]
    nvml: Option<Nvml>,
    #[serde(skip)]
    total_memory_mb: u64,
    #[serde(skip)]
    available_memory_mb: u64,
    state_path: PathBuf,
    next_job_id: u32,
    /// GPU indices that scheduler is allowed to use (None = all GPUs)
    allowed_gpu_indices: Option<Vec<u32>>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            log::error!(
                "Failed to create scheduler: {}. Creating minimal scheduler.",
                e
            );
            let total_memory_mb = Self::get_total_system_memory_mb();
            // Fallback to a minimal scheduler without GPU support
            Self {
                jobs: HashMap::new(),
                gpu_slots: HashMap::new(),
                nvml: None,
                total_memory_mb,
                available_memory_mb: total_memory_mb,
                state_path: PathBuf::from("state.json"),
                next_job_id: 1,
                allowed_gpu_indices: None,
            }
        })
    }
}

impl Scheduler {
    pub fn new() -> anyhow::Result<Self> {
        let state_path = get_data_dir()?.join("state.json");
        Self::with_state_path(state_path, None)
    }

    pub fn with_state_path(
        state_path: PathBuf,
        allowed_gpu_indices: Option<Vec<u32>>,
    ) -> anyhow::Result<Self> {
        // Try to initialize NVML, but continue without it if it fails
        let (nvml, gpu_slots) = match Nvml::init() {
            Ok(nvml) => {
                let gpu_slots = Self::get_gpus(&nvml);
                (Some(nvml), gpu_slots)
            }
            Err(e) => {
                log::warn!(
                    "Failed to initialize NVML: {}. Running without GPU support.",
                    e
                );
                (None, HashMap::new())
            }
        };

        // Validate allowed GPU indices
        if let Some(ref allowed) = allowed_gpu_indices {
            let detected_count = gpu_slots.len();
            let invalid: Vec<_> = allowed
                .iter()
                .filter(|&&idx| idx >= detected_count as u32)
                .copied()
                .collect();

            if !invalid.is_empty() {
                log::warn!(
                    "Invalid GPU indices {:?} specified (only {} GPUs detected). These will be ignored.",
                    invalid,
                    detected_count
                );
            }

            log::info!("GPU restriction enabled: allowing only GPUs {:?}", allowed);
        }

        let total_memory_mb = Self::get_total_system_memory_mb();
        let mut scheduler = Self {
            jobs: HashMap::new(),
            gpu_slots,
            nvml,
            total_memory_mb,
            available_memory_mb: total_memory_mb,
            state_path,
            next_job_id: 1,
            allowed_gpu_indices,
        };
        scheduler.load_state();
        Ok(scheduler)
    }

    /// Create a new scheduler without NVML initialization.
    /// This is useful for testing environments where NVML is not available.
    #[cfg(test)]
    pub fn new_without_nvml() -> Self {
        let state_path =
            std::env::temp_dir().join(format!("gflow_test_state_{}.json", std::process::id()));
        let total_memory_mb = Self::get_total_system_memory_mb();
        Self {
            jobs: HashMap::new(),
            gpu_slots: HashMap::new(),
            nvml: None,
            total_memory_mb,
            available_memory_mb: total_memory_mb,
            state_path,
            next_job_id: 1,
            allowed_gpu_indices: None,
        }
    }

    pub fn get_available_gpu_slots(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self
            .gpu_slots
            .values()
            .filter(|slot| slot.available)
            .map(|slot| slot.index)
            .filter(|&index| {
                // Apply GPU restriction filter
                match &self.allowed_gpu_indices {
                    None => true, // No restriction, all GPUs allowed
                    Some(allowed) => allowed.contains(&index),
                }
            })
            .collect();
        slots.sort_unstable();
        slots
    }

    pub fn info(&self) -> SchedulerInfo {
        let mut gpus: Vec<GpuInfo> = self
            .gpu_slots
            .iter()
            .map(|(uuid, slot)| GpuInfo {
                uuid: uuid.clone(),
                index: slot.index,
                available: slot.available,
            })
            .collect();
        // Sort by index for stable output
        gpus.sort_by_key(|g| g.index);
        SchedulerInfo {
            gpus,
            allowed_gpu_indices: self.allowed_gpu_indices.clone(),
        }
    }

    pub fn gpu_slots_count(&self) -> usize {
        self.gpu_slots.len()
    }

    pub fn set_allowed_gpu_indices(&mut self, indices: Option<Vec<u32>>) {
        self.allowed_gpu_indices = indices;
    }

    pub async fn submit_job(&mut self, mut job: Job) -> (u32, String) {
        job.id = self.next_job_id;
        self.next_job_id += 1;
        let job_ = Job {
            state: JobState::Queued,
            gpu_ids: None,
            run_name: job
                .run_name
                .or_else(|| Some(format!("gflow-job-{}", job.id))),
            ..job
        };
        let job_id = job_.id;
        let run_name = job_.run_name.clone().unwrap_or_default();
        self.jobs.insert(job_id, job_);
        self.save_state().await;
        (job_id, run_name)
    }

    pub async fn save_state(&self) {
        let path = &self.state_path;
        let tmp_path = path.with_extension("json.tmp");

        if let Ok(json) = serde_json::to_string_pretty(&self) {
            if let Ok(mut file) = tokio::fs::File::create(&tmp_path).await {
                if tokio::io::AsyncWriteExt::write_all(&mut file, json.as_bytes())
                    .await
                    .is_ok()
                {
                    // Atomic rename
                    tokio::fs::rename(&tmp_path, path).await.ok();
                }
            }
        }
    }

    pub fn load_state(&mut self) {
        let path = &self.state_path;
        if path.exists() {
            if let Ok(json) = std::fs::read_to_string(path) {
                if let Ok(mut scheduler) = serde_json::from_str::<Scheduler>(&json) {
                    // Preserve GPU restriction from current instance
                    // (CLI/config takes precedence over saved state)
                    scheduler.allowed_gpu_indices = self.allowed_gpu_indices.clone();

                    // Try to initialize NVML, but continue without it if it fails
                    match Nvml::init() {
                        Ok(nvml) => {
                            scheduler.gpu_slots = Self::get_gpus(&nvml);
                            scheduler.nvml = Some(nvml);
                        }
                        Err(e) => {
                            log::warn!("Failed to initialize NVML during state load: {}. Running without GPU support.", e);
                            scheduler.gpu_slots = HashMap::new();
                            scheduler.nvml = None;
                        }
                    }
                    // Initialize memory tracking
                    scheduler.total_memory_mb = Self::get_total_system_memory_mb();
                    scheduler.available_memory_mb = scheduler.total_memory_mb;
                    scheduler.refresh_available_memory();
                    *self = scheduler;
                }
            }
        }
    }

    pub fn refresh(&mut self) {
        self.refresh_gpu_slots();
        self.refresh_available_memory();
    }

    fn refresh_gpu_slots(&mut self) {
        let running_gpu_indices: std::collections::HashSet<u32> = self
            .jobs
            .values()
            .filter(|j| j.state == JobState::Running)
            .filter_map(|j| j.gpu_ids.as_ref())
            .flat_map(|ids| ids.iter().copied())
            .collect();

        if let Some(nvml) = &self.nvml {
            if let Ok(device_count) = nvml.device_count() {
                for i in 0..device_count {
                    if let Ok(device) = nvml.device_by_index(i) {
                        if let Ok(uuid) = device.uuid() {
                            if let Some(slot) = self.gpu_slots.get_mut(&uuid) {
                                let is_free_in_scheduler =
                                    !running_gpu_indices.contains(&slot.index);
                                let is_free_in_nvml = device
                                    .running_compute_processes()
                                    .is_ok_and(|procs| procs.is_empty());
                                slot.available = is_free_in_scheduler && is_free_in_nvml;
                            }
                        }
                    }
                }
            }
        }
    }

    pub async fn finish_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            let should_close_tmux = job.auto_close_tmux;
            let run_name = job.run_name.clone();

            job.try_transition(job_id, JobState::Finished);
            self.save_state().await;

            // Close tmux session if auto_close is enabled
            if should_close_tmux {
                if let Some(name) = run_name {
                    log::info!("Auto-closing tmux session '{}' for job {}", name, job_id);
                    if let Err(e) = gflow::tmux::kill_session(&name) {
                        log::warn!("Failed to auto-close tmux session '{}': {}", name, e);
                    }
                }
            }

            true
        } else {
            false
        }
    }

    pub async fn fail_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Failed);
            self.save_state().await;
            true
        } else {
            false
        }
    }

    pub async fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            // If the job is running, send Ctrl-C to gracefully interrupt it
            if job.state == JobState::Running {
                if let Some(run_name) = &job.run_name {
                    if let Err(e) = gflow::tmux::send_ctrl_c(run_name) {
                        log::error!("Failed to send C-c to tmux session {}: {}", run_name, e);
                    }
                }
            }
            job.try_transition(job_id, JobState::Cancelled);
            self.save_state().await;
            true
        } else {
            false
        }
    }

    pub async fn hold_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Hold);
            self.save_state().await;
            true
        } else {
            false
        }
    }

    pub async fn release_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Queued);
            self.save_state().await;
            true
        } else {
            false
        }
    }

    /// Resolve dependency shorthand to a job ID
    /// Supports formats:
    /// - "@" -> most recent submission by the user
    /// - "@~N" -> Nth most recent submission by the user
    ///   Returns None if shorthand is invalid or history is insufficient
    pub fn resolve_dependency(&self, username: &str, shorthand: &str) -> Option<u32> {
        let trimmed = shorthand.trim();

        if trimmed.is_empty() {
            return None;
        }

        // Get all jobs submitted by the user, sorted by job ID (which corresponds to submission order)
        let mut user_jobs: Vec<_> = self
            .jobs
            .values()
            .filter(|job| job.submitted_by == username)
            .collect();

        // Sort by job ID (ascending) since job IDs are assigned incrementally
        user_jobs.sort_by_key(|job| job.id);

        if trimmed == "@" {
            // Most recent submission
            return user_jobs.last().map(|job| job.id);
        }

        if let Some(offset_str) = trimmed.strip_prefix("@~") {
            if offset_str.is_empty() {
                return None;
            }
            let offset = offset_str.parse::<usize>().ok()?;
            if offset == 0 {
                return None;
            }
            if offset <= user_jobs.len() {
                return user_jobs.get(user_jobs.len() - offset).map(|job| job.id);
            }
        }

        None
    }

    /// Calculate time bonus for scheduling priority
    /// Returns a value between 100-300:
    /// - 100: No time limit (lowest bonus)
    /// - 200-300: Has time limit (shorter jobs get higher bonus)
    fn calculate_time_bonus(time_limit: &Option<Duration>) -> u32 {
        match time_limit {
            None => 100, // Jobs without time limits get lowest bonus
            Some(limit) => {
                // Normalize time limit against a 24-hour maximum
                let max_time_secs = 24.0 * 3600.0; // 24 hours in seconds
                let limit_secs = limit.as_secs_f64();
                let normalized = (limit_secs / max_time_secs).min(1.0);

                // Shorter jobs get higher bonus (up to 300)
                // Longer jobs get lower bonus (down to 200)
                // Formula: 200 + (1 - normalized) * 100
                200 + ((1.0 - normalized) * 100.0) as u32
            }
        }
    }

    /// Get total system memory in MB by reading /proc/meminfo (Linux)
    /// Returns a default value if reading fails
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

        // Fallback: assume 16GB if we can't read system memory
        log::warn!("Could not read system memory from /proc/meminfo, assuming 16GB");
        16 * 1024
    }

    /// Refresh available memory by calculating memory used by running jobs
    fn refresh_available_memory(&mut self) {
        let memory_used: u64 = self
            .jobs
            .values()
            .filter(|j| j.state == JobState::Running)
            .filter_map(|j| j.memory_limit_mb)
            .sum();

        self.available_memory_mb = self.total_memory_mb.saturating_sub(memory_used);
    }
}

impl GPU for Scheduler {
    fn get_gpus(nvml: &Nvml) -> HashMap<UUID, GPUSlot> {
        let mut gpu_slots = HashMap::new();
        let device_count = nvml.device_count().unwrap_or(0);
        for i in 0..device_count {
            if let Ok(device) = nvml.device_by_index(i) {
                if let Ok(uuid) = device.uuid() {
                    gpu_slots.insert(
                        uuid,
                        GPUSlot {
                            available: true,
                            index: i,
                        },
                    );
                }
            }
        }
        gpu_slots
    }
}

pub async fn run(shared_state: SharedState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;

        // Step 1: Refresh GPU slots (write lock - very brief)
        {
            let mut state = shared_state.write().await;
            state.refresh();
        }

        // Step 2: Detect zombie jobs (collect data with read lock, check tmux without lock)
        let running_jobs = {
            let state = shared_state.read().await;
            state
                .jobs
                .values()
                .filter(|j| j.state == JobState::Running)
                .map(|j| (j.id, j.run_name.clone()))
                .collect::<Vec<_>>()
        };

        // Check tmux sessions without holding any lock
        let mut zombie_job_ids = Vec::new();
        for (job_id, run_name) in running_jobs {
            if let Some(rn) = run_name {
                if !gflow::tmux::is_session_exist(&rn) {
                    log::warn!("Found zombie job (id: {}), marking as Failed.", job_id);
                    zombie_job_ids.push(job_id);
                }
            }
        }

        // Update zombie jobs (write lock - brief)
        if !zombie_job_ids.is_empty() {
            let mut state = shared_state.write().await;
            for job_id in zombie_job_ids {
                if let Some(job) = state.jobs.get_mut(&job_id) {
                    job.state = JobState::Failed;
                    job.finished_at = Some(std::time::SystemTime::now());
                }
            }
            state.save_state().await;
        }

        // Step 3: Check for timed-out jobs (read lock to identify, then handle)
        let timed_out_jobs = {
            let state = shared_state.read().await;
            state
                .jobs
                .values()
                .filter(|job| job.has_exceeded_time_limit())
                .map(|job| {
                    log::warn!("Job {} has exceeded time limit, terminating...", job.id);
                    (job.id, job.run_name.clone())
                })
                .collect::<Vec<_>>()
        };

        // Terminate timed-out jobs without lock, then update state
        for (job_id, run_name) in timed_out_jobs {
            if let Some(run_name) = run_name {
                if let Err(e) = gflow::tmux::send_ctrl_c(&run_name) {
                    log::error!("Failed to send C-c to timed-out job {}: {}", job_id, e);
                }
            }

            // Update job state (brief write lock per job)
            let mut state = shared_state.write().await;
            if let Some(job) = state.jobs.get_mut(&job_id) {
                job.try_transition(job_id, JobState::Timeout);
            }
            state.save_state().await;
        }

        // Step 4: Schedule and execute new jobs (write lock for scheduling decision)
        let mut state = shared_state.write().await;

        let mut available_gpus = state.get_available_gpu_slots();
        let finished_jobs: std::collections::HashSet<u32> = state
            .jobs
            .values()
            .filter(|j| j.state == JobState::Finished)
            .map(|j| j.id)
            .collect();

        // Collect and sort runnable jobs
        let mut runnable_jobs: Vec<_> = state
            .jobs
            .values()
            .filter(|j| j.state == JobState::Queued)
            .filter(|j| {
                if let Some(dependency_id) = j.depends_on {
                    return finished_jobs.contains(&dependency_id);
                }
                true
            })
            .map(|j| j.id)
            .collect();

        runnable_jobs.sort_by_key(|job_id| {
            state
                .jobs
                .get(job_id)
                .map(|job| {
                    let time_bonus = Scheduler::calculate_time_bonus(&job.time_limit);
                    std::cmp::Reverse((job.priority, time_bonus, std::cmp::Reverse(job.id)))
                })
                .unwrap_or(std::cmp::Reverse((0, 0, std::cmp::Reverse(*job_id))))
        });

        // Execute runnable jobs
        let available_memory = state.available_memory_mb;
        for job_id in runnable_jobs {
            if let Some(job) = state.jobs.get_mut(&job_id) {
                // Check if sufficient GPUs are available
                let has_enough_gpus = job.gpus as usize <= available_gpus.len();

                // Check if sufficient memory is available
                // If job has no memory limit, treat it as requiring 0 MB
                let required_memory = job.memory_limit_mb.unwrap_or(0);
                let has_enough_memory = required_memory <= available_memory;

                // Only execute if both GPU and memory requirements are met
                if has_enough_gpus && has_enough_memory {
                    let gpus_for_job = available_gpus
                        .drain(..job.gpus as usize)
                        .collect::<Vec<_>>();
                    job.gpu_ids = Some(gpus_for_job);

                    let executor = TmuxExecutor;
                    match executor.execute(job) {
                        Ok(_) => {
                            job.state = JobState::Running;
                            job.started_at = Some(std::time::SystemTime::now());
                            log::info!("Executing job: {job:?}");
                            // Reserve memory for this job
                            state.available_memory_mb =
                                state.available_memory_mb.saturating_sub(required_memory);
                        }
                        Err(e) => {
                            log::error!("Failed to execute job: {e:?}");
                            job.state = JobState::Failed;
                        }
                    }
                } else if !has_enough_memory {
                    log::debug!(
                        "Job {} waiting for memory: needs {}MB, available {}MB",
                        job.id,
                        required_memory,
                        available_memory
                    );
                }
            }
        }
        // Write lock released here
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::{Job, JobBuilder};

    fn create_test_job(id: u32, username: &str) -> Job {
        let mut job = JobBuilder::new()
            .submitted_by(username.to_string())
            .run_dir("/tmp")
            .build();
        job.id = id;
        job
    }

    #[test]
    fn test_resolve_dependency_most_recent() {
        let mut scheduler = Scheduler::new_without_nvml();
        let username = "testuser";

        // Add some jobs for the user
        let job1 = create_test_job(1, username);
        let job2 = create_test_job(2, username);
        let job3 = create_test_job(3, username);

        scheduler.jobs.insert(1, job1);
        scheduler.jobs.insert(2, job2);
        scheduler.jobs.insert(3, job3);

        // Test resolving "@" (most recent)
        let resolved = scheduler.resolve_dependency(username, "@");
        assert_eq!(resolved, Some(3));
    }

    #[test]
    fn test_resolve_dependency_with_offset() {
        let mut scheduler = Scheduler::new_without_nvml();
        let username = "testuser";

        // Add some jobs for the user
        let job1 = create_test_job(1, username);
        let job2 = create_test_job(2, username);
        let job3 = create_test_job(3, username);
        let job4 = create_test_job(4, username);

        scheduler.jobs.insert(1, job1);
        scheduler.jobs.insert(2, job2);
        scheduler.jobs.insert(3, job3);
        scheduler.jobs.insert(4, job4);

        // Test resolving "@~2" (2nd most recent)
        let resolved = scheduler.resolve_dependency(username, "@~2");
        assert_eq!(resolved, Some(3));

        // Test resolving "@~3" (3rd most recent)
        let resolved = scheduler.resolve_dependency(username, "@~3");
        assert_eq!(resolved, Some(2));

        // Test resolving "@~4" (4th most recent)
        let resolved = scheduler.resolve_dependency(username, "@~4");
        assert_eq!(resolved, Some(1));
    }

    #[test]
    fn test_resolve_dependency_per_user() {
        let mut scheduler = Scheduler::new_without_nvml();

        // Add jobs for different users
        let job1 = create_test_job(1, "alice");
        let job2 = create_test_job(2, "bob");
        let job3 = create_test_job(3, "alice");
        let job4 = create_test_job(4, "bob");

        scheduler.jobs.insert(1, job1);
        scheduler.jobs.insert(2, job2);
        scheduler.jobs.insert(3, job3);
        scheduler.jobs.insert(4, job4);

        // Alice should see her most recent job (3)
        let resolved = scheduler.resolve_dependency("alice", "@");
        assert_eq!(resolved, Some(3));

        // Bob should see his most recent job (4)
        let resolved = scheduler.resolve_dependency("bob", "@");
        assert_eq!(resolved, Some(4));
    }

    #[test]
    fn test_resolve_dependency_not_found() {
        let scheduler = Scheduler::new_without_nvml();

        // Test with no jobs
        let resolved = scheduler.resolve_dependency("nonexistent", "@");
        assert_eq!(resolved, None);

        // Test with invalid offset
        let resolved = scheduler.resolve_dependency("testuser", "@~0");
        assert_eq!(resolved, None);

        // Test with invalid shorthand
        let resolved = scheduler.resolve_dependency("testuser", "@foo");
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_calculate_time_bonus_no_limit() {
        // Jobs without time limits should get the lowest bonus
        assert_eq!(Scheduler::calculate_time_bonus(&None), 100);
    }

    #[test]
    fn test_calculate_time_bonus_short_job() {
        // Very short jobs (1 minute) should get close to maximum bonus
        let one_minute = Duration::from_secs(60);
        let bonus = Scheduler::calculate_time_bonus(&Some(one_minute));
        assert!(bonus >= 299, "Expected bonus >= 299, got {}", bonus);
        assert!(bonus <= 300, "Expected bonus <= 300, got {}", bonus);
    }

    #[test]
    fn test_calculate_time_bonus_medium_job() {
        // Medium jobs (1 hour) should get intermediate bonus
        let one_hour = Duration::from_secs(3600);
        let bonus = Scheduler::calculate_time_bonus(&Some(one_hour));
        assert!(
            bonus > 200 && bonus < 300,
            "Expected 200 < bonus < 300, got {}",
            bonus
        );
    }

    #[test]
    fn test_calculate_time_bonus_long_job() {
        // Long jobs (24 hours) should get minimum time-limited bonus
        let twenty_four_hours = Duration::from_secs(24 * 3600);
        let bonus = Scheduler::calculate_time_bonus(&Some(twenty_four_hours));
        assert_eq!(bonus, 200);
    }

    #[test]
    fn test_calculate_time_bonus_very_long_job() {
        // Jobs longer than 24 hours should still get 200 (min for time-limited jobs)
        let forty_eight_hours = Duration::from_secs(48 * 3600);
        let bonus = Scheduler::calculate_time_bonus(&Some(forty_eight_hours));
        assert_eq!(bonus, 200);
    }

    #[test]
    fn test_time_bonus_ordering() {
        // Shorter jobs should have higher bonus than longer jobs
        let one_min = Some(Duration::from_secs(60));
        let one_hour = Some(Duration::from_secs(3600));
        let one_day = Some(Duration::from_secs(24 * 3600));

        let bonus_1min = Scheduler::calculate_time_bonus(&one_min);
        let bonus_1hr = Scheduler::calculate_time_bonus(&one_hour);
        let bonus_1day = Scheduler::calculate_time_bonus(&one_day);
        let bonus_none = Scheduler::calculate_time_bonus(&None);

        assert!(
            bonus_1min > bonus_1hr,
            "1 minute should have higher bonus than 1 hour"
        );
        assert!(
            bonus_1hr > bonus_1day,
            "1 hour should have higher bonus than 1 day"
        );
        assert!(
            bonus_1day > bonus_none,
            "1 day time limit should have higher bonus than no limit"
        );
    }
}
