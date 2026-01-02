use crate::core::job::{Job, JobState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct DebugState {
    pub jobs: HashMap<u32, Job>,
    pub next_job_id: u32,
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
    pub gpu_slots: Vec<DebugGpuSlot>,
    pub allowed_gpu_indices: Option<Vec<u32>>,
}

#[derive(Serialize, Deserialize)]
pub struct DebugGpuSlot {
    pub uuid: String,
    pub index: u32,
    pub available: bool,
}

#[derive(Serialize, Deserialize)]
pub struct DebugJobInfo {
    #[serde(flatten)]
    pub job: Job,
    pub runtime_seconds: Option<f64>,
    pub time_remaining_seconds: Option<f64>,
}

impl DebugJobInfo {
    pub fn from_job(job: Job) -> Self {
        let runtime_seconds = job.started_at.and_then(|start| {
            std::time::SystemTime::now()
                .duration_since(start)
                .ok()
                .map(|d| d.as_secs_f64())
        });

        let time_remaining_seconds = match (job.time_limit, runtime_seconds) {
            (Some(limit), Some(runtime)) => Some((limit.as_secs_f64() - runtime).max(0.0)),
            _ => None,
        };

        Self {
            job,
            runtime_seconds,
            time_remaining_seconds,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct DebugMetrics {
    pub jobs_by_state: HashMap<JobState, usize>,
    pub jobs_by_user: HashMap<String, UserJobStats>,
}

#[derive(Serialize, Deserialize)]
pub struct UserJobStats {
    pub submitted: usize,
    pub running: usize,
    pub finished: usize,
    pub failed: usize,
}
