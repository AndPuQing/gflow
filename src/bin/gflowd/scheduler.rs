use crate::executor::TmuxExecutor;
use gflow::core::executor::Executor;
use gflow::core::get_data_dir;
use gflow::core::{
    job::{Job, JobState},
    GPUSlot, GPU, UUID,
};
use nvml_wrapper::Nvml;
use std::{collections::HashMap, fs::File, io::Write, path::PathBuf, sync::Arc, time::Duration};
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
    state_path: PathBuf,
    next_job_id: u32,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        // This is not ideal, but for now we will panic if we can't get the config dir
        let state_path = get_data_dir().unwrap().join("state.json");
        Self::with_state_path(state_path)
    }

    pub fn with_state_path(state_path: PathBuf) -> Self {
        let nvml = Nvml::init().expect("Failed to initialize NVML");
        let gpu_slots = Self::get_gpus(&nvml);
        let mut scheduler = Self {
            jobs: HashMap::new(),
            gpu_slots,
            nvml: Some(nvml),
            state_path,
            next_job_id: 1,
        };
        scheduler.load_state();
        scheduler
    }

    pub fn get_available_gpu_slots(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self
            .gpu_slots
            .values()
            .filter(|slot| slot.available)
            .map(|slot| slot.index)
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
        SchedulerInfo { gpus }
    }

    pub fn submit_job(&mut self, mut job: Job) -> (u32, String) {
        job.id = self.next_job_id;
        self.next_job_id += 1;
        let job_ = Job {
            state: JobState::Queued,
            gpu_ids: None,
            run_name: Some(gflow::core::random_run_name()),
            ..job
        };
        let job_id = job_.id;
        let run_name = job_.run_name.clone().unwrap_or_default();
        self.jobs.insert(job_id, job_);
        self.save_state();
        (job_id, run_name)
    }

    pub fn save_state(&self) {
        let path = &self.state_path;
        let tmp_path = path.with_extension("json.tmp");

        if let Ok(json) = serde_json::to_string_pretty(&self) {
            if let Ok(mut file) = File::create(&tmp_path) {
                if file.write_all(json.as_bytes()).is_ok() {
                    // Atomic rename
                    std::fs::rename(&tmp_path, path).ok();
                }
            }
        }
    }

    pub fn load_state(&mut self) {
        let path = &self.state_path;
        if path.exists() {
            if let Ok(json) = std::fs::read_to_string(path) {
                if let Ok(mut scheduler) = serde_json::from_str::<Scheduler>(&json) {
                    scheduler.nvml = Some(Nvml::init().expect("Failed to initialize NVML"));
                    scheduler.gpu_slots = Self::get_gpus(scheduler.nvml.as_ref().unwrap());
                    *self = scheduler;
                }
            }
        }
    }

    pub fn refresh(&mut self) {
        self.refresh_gpu_slots();
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

    pub fn finish_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Finished);
            self.save_state();
            true
        } else {
            false
        }
    }

    pub fn fail_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Failed);
            self.save_state();
            true
        } else {
            false
        }
    }

    pub fn cancel_job(&mut self, job_id: u32) -> bool {
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
            self.save_state();
            true
        } else {
            false
        }
    }

    pub fn hold_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Hold);
            self.save_state();
            true
        } else {
            false
        }
    }

    pub fn release_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.try_transition(job_id, JobState::Queued);
            self.save_state();
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
        let mut state = shared_state.write().await;
        state.refresh(); // Refresh based on NVML

        // Detect and clean up zombie jobs
        let mut zombie_jobs_found = false;
        for job in state.jobs.values_mut() {
            if job.state == JobState::Running {
                if let Some(run_name) = &job.run_name {
                    let session_exists = gflow::tmux::is_session_exist(run_name);
                    if !session_exists {
                        log::warn!("Found zombie job (id: {}), marking as Failed.", job.id);
                        job.state = JobState::Failed;
                        job.finished_at = Some(std::time::SystemTime::now());
                        zombie_jobs_found = true;
                    }
                }
            }
        }
        if zombie_jobs_found {
            state.save_state();
        }

        // Check for timed-out jobs
        let mut timed_out_jobs = Vec::new();
        for job in state.jobs.values() {
            if job.has_exceeded_time_limit() {
                log::warn!("Job {} has exceeded time limit, terminating...", job.id);
                timed_out_jobs.push((job.id, job.run_name.clone()));
            }
        }

        // Terminate timed-out jobs
        for (job_id, run_name) in timed_out_jobs {
            if let Some(run_name) = run_name {
                // Send Ctrl-C to interrupt the job
                if let Err(e) = gflow::tmux::send_ctrl_c(&run_name) {
                    log::error!("Failed to send C-c to timed-out job {}: {}", job_id, e);
                }
            }
            // Mark job as timed out
            if let Some(job) = state.jobs.get_mut(&job_id) {
                job.try_transition(job_id, JobState::Timeout);
            }
            state.save_state();
        }

        let mut available_gpus = state.get_available_gpu_slots();

        let finished_jobs: std::collections::HashSet<u32> = state
            .jobs
            .values()
            .filter(|j| j.state == JobState::Finished)
            .map(|j| j.id)
            .collect();

        // Sort all queued jobs by priority, time limit, and submission order
        // Priority hierarchy:
        // 1. User priority (highest priority wins)
        // 2. Time limit bonus (time-limited jobs preferred, shorter jobs first)
        // 3. Submission order (earlier submissions first, using job ID as proxy)
        // Note: Held jobs are not eligible for scheduling
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
            let job = state.jobs.get(job_id).unwrap();
            let time_bonus = Scheduler::calculate_time_bonus(&job.time_limit);
            // Tuples compare lexicographically: priority first, then time_bonus, then job_id
            // Use Reverse for descending order on priority and time_bonus
            // Use double Reverse on job_id for ascending order (FIFO)
            std::cmp::Reverse((job.priority, time_bonus, std::cmp::Reverse(job.id)))
        });

        // Easy backfilling loop
        for job_id in runnable_jobs {
            if let Some(job) = state.jobs.get_mut(&job_id) {
                if job.gpus as usize <= available_gpus.len() {
                    // This job can run
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
                        }
                        Err(e) => {
                            log::error!("Failed to execute job: {e:?}");
                            job.state = JobState::Failed;
                        }
                    }
                }
            }
        }
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
        let mut scheduler = Scheduler::new();
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
        let mut scheduler = Scheduler::new();
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
        let mut scheduler = Scheduler::new();

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
        let scheduler = Scheduler::new();

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
