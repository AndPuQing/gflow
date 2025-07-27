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
        let state_path = get_config_temp_dir().join("state.json");
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

    pub fn get_available_gpu_slots(&self) -> Vec<u32> {
        self.gpu_slots
            .iter()
            .filter_map(|(_uuid, slot)| {
                if slot.available {
                    Some(slot.index)
                } else {
                    None
                }
            })
            .collect()
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
        if let Ok(json) = serde_json::to_string_pretty(&self) {
            if let Ok(mut file) = File::create(path) {
                file.write_all(json.as_bytes()).ok();
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

        // Best fit algorithm
        let mut available_gpus = state.get_available_gpu_slots();
        let job = state.jobs.iter_mut().find(|job| {
            job.state == JobState::Queued
                && !available_gpus.is_empty()
                && job.gpus <= available_gpus.len() as u32
        });

        if let Some(job) = job {
            available_gpus.truncate(job.gpus as usize);
            job.gpu_ids = Some(available_gpus.clone());
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
