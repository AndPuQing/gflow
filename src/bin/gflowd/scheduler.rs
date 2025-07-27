use crate::executor::TmuxExecutor;
use gflow::core::executor::Executor;
use gflow::core::get_config_temp_dir;
use gflow::core::{
    job::{Job, JobState},
    GPUSlot, GPU, UUID,
};
use nvml_wrapper::Nvml;
use std::{collections::HashMap, fs::File, io::Write, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub type SharedState = Arc<Mutex<Scheduler>>;

use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Scheduler {
    pub jobs: Vec<Job>,
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
        let state_path = get_config_temp_dir().unwrap().join("state.json");
        Self::with_state_path(state_path)
    }

    pub fn with_state_path(state_path: PathBuf) -> Self {
        let nvml = Nvml::init().expect("Failed to initialize NVML");
        let gpu_slots = Self::get_gpus(&nvml);
        let mut scheduler = Self {
            jobs: Vec::new(),
            gpu_slots,
            nvml: Some(nvml),
            state_path,
            next_job_id: 1,
        };
        scheduler.load_state();
        scheduler
    }

    pub fn get_available_gpu_slots(&self) -> Vec<(u32, u64)> {
        let mut slots = Vec::new();
        if let Some(nvml) = &self.nvml {
            for (uuid, slot) in &self.gpu_slots {
                if slot.available {
                    if let Ok(device) = nvml.device_by_uuid(uuid.clone()) {
                        if let Ok(mem_info) = device.memory_info() {
                            // a bit of a hack to get the free memory in MB
                            slots.push((slot.index, mem_info.free / 1024 / 1024));
                        }
                    }
                }
            }
        }
        slots
    }

    pub fn info(&self) -> HashMap<String, Vec<u32>> {
        self.gpu_slots
            .iter()
            .map(|(uuid, slot)| (uuid.clone(), vec![slot.index]))
            .collect()
    }

    pub fn submit_job(&mut self, mut job: Job) {
        job.id = self.next_job_id;
        self.next_job_id += 1;
        let job_ = Job {
            state: JobState::Queued,
            gpu_ids: None,
            run_name: Some(gflow::core::random_run_name()),
            ..job
        };
        self.jobs.push(job_);
        self.save_state();
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
        if let Some(nvml) = &self.nvml {
            if let Ok(device_count) = nvml.device_count() {
                for i in 0..device_count {
                    if let Ok(device) = nvml.device_by_index(i) {
                        if let Ok(uuid) = device.uuid() {
                            if let Some(slot) = self.gpu_slots.get_mut(&uuid) {
                                slot.available = device
                                    .running_compute_processes()
                                    .is_ok_and(|procs| procs.is_empty());
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn finish_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.state = JobState::Finished;
            self.save_state();
            true
        } else {
            false
        }
    }

    pub fn fail_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.state = JobState::Failed;
            self.save_state();
            true
        } else {
            false
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
        let mut state = shared_state.lock().await;
        state.refresh();

        // Detect and clean up zombie jobs
        let mut zombie_jobs_found = false;
        for job in &mut state.jobs {
            if job.state == JobState::Running {
                if let Some(run_name) = &job.run_name {
                    let session_exists = gflow::tmux::is_session_exist(run_name);
                    if !session_exists {
                        log::warn!("Found zombie job (id: {}), marking as Failed.", job.id);
                        job.state = JobState::Failed;
                        zombie_jobs_found = true;
                    }
                }
            }
        }
        if zombie_jobs_found {
            state.save_state();
        }

        let available_gpus_with_mem = state.get_available_gpu_slots();

        // Find the highest priority job that can be run
        let finished_jobs: std::collections::HashSet<u32> = state
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Finished)
            .map(|j| j.id)
            .collect();

        let job_to_run = state
            .jobs
            .iter_mut()
            .filter(|j| j.state == JobState::Queued)
            .filter(|j| {
                // Check for dependencies
                if let Some(dependency_id) = j.depends_on {
                    if !finished_jobs.contains(&dependency_id) {
                        return false; // Dependency not met
                    }
                }

                let suitable_gpus: Vec<_> = available_gpus_with_mem
                    .iter()
                    .filter(|(_, free_mem)| *free_mem >= j.gpu_mem)
                    .collect();
                j.gpus <= suitable_gpus.len() as u32
            })
            .max_by_key(|j| j.priority);

        if let Some(job) = job_to_run {
            let mut suitable_gpus: Vec<_> = available_gpus_with_mem
                .iter()
                .filter(|(_, free_mem)| *free_mem >= job.gpu_mem)
                .map(|(index, _)| *index)
                .collect();

            suitable_gpus.truncate(job.gpus as usize);
            job.gpu_ids = Some(suitable_gpus);
            let executor = TmuxExecutor;
            match executor.execute(job) {
                Ok(_) => {
                    job.state = JobState::Running;
                    log::info!("Executing job: {:?}", job);
                }
                Err(e) => {
                    log::error!("Failed to execute job: {:?}", e);
                    job.state = JobState::Failed;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::JobBuilder;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_submit_job() {
        let dir = tempdir().unwrap();
        let state_path = dir.path().join("state.json");
        let mut scheduler = Scheduler::with_state_path(state_path);
        let job = JobBuilder::new().script(PathBuf::from("test.sh")).build();
        scheduler.submit_job(job);
        assert_eq!(scheduler.jobs.len(), 1);
        assert_eq!(scheduler.jobs[0].state, JobState::Queued);
    }

    #[test]
    fn test_save_and_load_state() {
        let dir = tempdir().unwrap();
        let state_path = dir.path().join("state.json");
        let mut scheduler = Scheduler::with_state_path(state_path.clone());

        let job = JobBuilder::new().script(PathBuf::from("test.sh")).build();
        scheduler.submit_job(job);

        scheduler.save_state();

        let new_scheduler = Scheduler::with_state_path(state_path);
        assert_eq!(new_scheduler.jobs.len(), 1);
        assert_eq!(new_scheduler.jobs[0].script, Some(PathBuf::from("test.sh")));
    }
}
