use crate::executor::TmuxExecutor;
use gflow::core::executor::Executor;
use gflow::core::get_data_dir;
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
        let state_path = get_data_dir().unwrap().join("state.json");
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
        self.jobs.push(job_.clone());
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
            .iter()
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
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.state = JobState::Finished;
            job.finished_at = Some(std::time::SystemTime::now());
            self.save_state();
            true
        } else {
            false
        }
    }

    pub fn fail_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.state = JobState::Failed;
            job.finished_at = Some(std::time::SystemTime::now());
            self.save_state();
            true
        } else {
            false
        }
    }

    pub fn cancel_job(&mut self, job_id: u32) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            // Can only cancel queued or running jobs
            if job.state == JobState::Queued || job.state == JobState::Running {
                // If the job is running, send Ctrl-C to gracefully interrupt it
                if job.state == JobState::Running {
                    if let Some(run_name) = &job.run_name {
                        if let Err(e) = gflow::tmux::send_ctrl_c(run_name) {
                            log::error!("Failed to send C-c to tmux session {}: {}", run_name, e);
                        }
                    }
                }
                job.state = JobState::Cancelled;
                job.finished_at = Some(std::time::SystemTime::now());
                self.save_state();
                true
            } else {
                false
            }
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
        state.refresh(); // Refresh based on NVML

        // Detect and clean up zombie jobs
        let mut zombie_jobs_found = false;
        for job in &mut state.jobs {
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

        let mut available_gpus = state.get_available_gpu_slots();

        let finished_jobs: std::collections::HashSet<u32> = state
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Finished)
            .map(|j| j.id)
            .collect();

        // Sort all queued jobs by priority
        let mut runnable_jobs_indices: Vec<_> = state
            .jobs
            .iter()
            .enumerate()
            .filter(|(_, j)| j.state == JobState::Queued)
            .filter(|(_, j)| {
                if let Some(dependency_id) = j.depends_on {
                    return finished_jobs.contains(&dependency_id);
                }
                true
            })
            .map(|(i, _)| i)
            .collect();

        runnable_jobs_indices.sort_by_key(|&i| std::cmp::Reverse(state.jobs[i].priority));

        // Easy backfilling loop
        for index in runnable_jobs_indices {
            if let Some(job) = state.jobs.get_mut(index) {
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
