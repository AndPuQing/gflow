use crate::job::execute_job;
use gflow::{
    job::{Job, JobState},
    GPUSlot, GPU, UUID,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub type SharedState = Arc<Mutex<Scheduler>>;

#[derive(Debug)]
pub struct Scheduler {
    pub jobs: Vec<Job>,
    gpu_slots: HashMap<UUID, GPUSlot>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        let gpu_slots = Self::get_gpus();
        Self {
            jobs: Vec::new(),
            gpu_slots,
        }
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

    pub fn submit_job(&mut self, job: Job) {
        let job_ = Job {
            state: JobState::Queued,
            gpu_ids: None,
            run_name: Some(gflow::random_run_name()),
            ..job
        };
        self.jobs.push(job_);
    }

    pub fn refresh(&mut self) {
        self.refresh_gpu_slots();
    }

    fn refresh_gpu_slots(&mut self) {
        let output = std::process::Command::new("nvidia-smi")
            .args(["--query-compute-apps=gpu_uuid,pid", "--format=csv,noheader"])
            .output();

        let mut gpu_processes: HashMap<String, Vec<u32>> = HashMap::new();

        if let Ok(output) = output {
            let output = String::from_utf8_lossy(&output.stdout);
            for line in output.lines() {
                let parts: Vec<&str> = line.split(", ").collect();
                if parts.len() == 2 {
                    let uuid = parts[0].to_string();
                    let pid = parts[1].parse::<u32>().unwrap_or(0);
                    gpu_processes.entry(uuid).or_default().push(pid);
                }
            }
        }
        // Update the availability of each GPU slot
        for (uuid, slot) in self.gpu_slots.iter_mut() {
            slot.available = !gpu_processes.contains_key(uuid);
        }
    }
}

impl GPU for Scheduler {
    fn get_gpus() -> HashMap<UUID, GPUSlot> {
        match std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=gpu_uuid,index", "--format=csv,noheader"])
            .output()
        {
            Ok(output) => {
                let output = String::from_utf8_lossy(&output.stdout);
                let mut gpu_slots = HashMap::new();
                for line in output.lines() {
                    let parts: Vec<&str> = line.split(", ").collect();
                    if parts.len() == 2 {
                        let gpu_uuid = parts[0].to_string();
                        let index = parts[1].parse::<u32>().unwrap_or(0);

                        gpu_slots.insert(
                            gpu_uuid,
                            GPUSlot {
                                available: true,
                                index,
                            },
                        );
                    }
                }
                gpu_slots
            }
            Err(_) => HashMap::new(),
        }
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
            match execute_job(job) {
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
