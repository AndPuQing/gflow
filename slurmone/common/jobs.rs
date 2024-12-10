use serde::{Deserialize, Serialize};
use std::{fs::read_to_string, path::Path};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl From<String> for Priority {
    fn from(priority: String) -> Self {
        match priority.to_lowercase().as_str() {
            "low" => Priority::Low,
            "normal" => Priority::Medium,
            "high" => Priority::High,
            _ => Priority::Medium,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Job {
    pub id: usize,
    pub user: String,
    pub command: String,
    pub priority: Priority,
    pub environment: JobEnvironment,
    pub status: JobStatus,
}

impl Job {
    pub fn execute(&mut self, _gpu_ids: Vec<usize>) {
        // 执行任务
        self.status = JobStatus::Running;
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JobEnvironment {
    pub job_name: Option<String>,
    pub gpus: Option<usize>,
    pub conda_env: Option<String>,
    pub work_dir: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub time_limit: Option<String>,
}

impl JobEnvironment {
    pub fn parse_slurm_script(path: &Path) -> Result<JobEnvironment, Box<dyn std::error::Error>> {
        let content = read_to_string(path)?;
        let mut config = JobEnvironment::default();

        for line in content.lines() {
            if line.starts_with("#SBATCH") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let key = parts[1].trim_start_matches("--").to_string();
                    let value = parts[2..].join(" ");
                    match key.as_str() {
                        "job-name" => config.job_name = Some(value),
                        "gres" => {
                            let gres: Vec<&str> = value.split(':').collect();
                            if gres.len() == 2 {
                                config.gpus = Some(gres[1].parse()?);
                            }
                        }
                        "time" => config.time_limit = Some(value),
                        "output" => config.output = Some(value),
                        "error" => config.error = Some(value),
                        "workdir" => config.work_dir = Some(value),
                        "conda-env" => config.conda_env = Some(value),
                        _ => {}
                    }
                }
            }
        }

        Ok(config)
    }
}

#[derive(Debug)]
pub struct Gpu {
    pub id: usize,
    pub is_busy: bool,
}

pub struct GPUManager {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Canceled,
    Hold,
}

impl Default for JobStatus {
    fn default() -> Self {
        JobStatus::Pending
    }
}

impl From<String> for JobStatus {
    fn from(status: String) -> Self {
        match status.to_lowercase().as_str() {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "canceled" => JobStatus::Canceled,
            _ => JobStatus::Pending,
        }
    }
}
