use shared::{Job, JobState, GPU};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

use crate::job::execute_job;

pub type SharedState = Arc<Mutex<Scheduler>>;

#[derive(Debug)]
pub struct GPUSlot {
    available: bool,
}

#[derive(Debug)]
pub struct Scheduler {
    pub jobs: Vec<Job>,
    gpu_slots: HashMap<u32, GPUSlot>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        let gpu_count = Self::get_gpu_count();
        let mut gpu_slots = HashMap::new();
        for i in 0..gpu_count {
            gpu_slots.insert(i, GPUSlot { available: true });
        }
        Self {
            jobs: Vec::new(),
            gpu_slots,
        }
    }

    pub fn get_available_gpu_slots(&self) -> Vec<u32> {
        self.gpu_slots
            .iter()
            .filter_map(|(gpu_id, slot)| if slot.available { Some(*gpu_id) } else { None })
            .collect()
    }

    pub fn submit_job(&mut self, job: Job) {
        self.jobs.push(job);
    }

    pub fn refresh(&mut self) {
        self.refresh_gpu_slots();
    }

    fn refresh_gpu_slots(&mut self) {
        let output = std::process::Command::new("nvidia-smi")
            .args(["--query-compute-apps=gpu_uuid,pid", "--format=csv,noheader"])
            .output();

        let mut gpu_processes: HashMap<u32, Vec<u32>> = HashMap::new();

        if let Ok(output) = output {
            let output = String::from_utf8_lossy(&output.stdout);
            for line in output.lines() {
                let parts: Vec<&str> = line.split(", ").collect();
                if parts.len() == 2 {
                    let gpu_id = parts[0].parse::<u32>().unwrap_or(0);
                    let pid = parts[1].parse::<u32>().unwrap_or(0);
                    gpu_processes.entry(gpu_id).or_default().push(pid);
                }
            }
        }
        // Update the availability of each GPU slot
        for (gpu_id, slot) in self.gpu_slots.iter_mut() {
            slot.available = !gpu_processes.contains_key(gpu_id)
        }
    }
}

impl GPU for Scheduler {
    fn get_gpu_count() -> u32 {
        match std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=gpu_name", "--format=csv,noheader"])
            .output()
        {
            Ok(output) => String::from_utf8_lossy(&output.stdout).lines().count() as u32,
            Err(_) => 0, // Return 0 if nvidia-smi fails or is not available
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
            log::info!("Executing job: {:?}", job);
            available_gpus.truncate(job.gpus as usize);
            execute_job(job, &available_gpus);
        }
    }
}
