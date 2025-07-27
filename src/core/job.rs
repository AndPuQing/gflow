use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use strum::{Display, EnumIter, FromRepr};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone, Display, EnumIter, FromRepr)]
pub enum JobState {
    #[strum(to_string = "Queued")]
    Queued,
    #[strum(to_string = "Running")]
    Running,
    #[strum(to_string = "Finished")]
    Finished,
    #[strum(to_string = "Failed")]
    Failed,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Job {
    /// Required fields at submission time
    pub id: u32,
    pub script: Option<PathBuf>,
    pub command: Option<String>,
    pub gpus: u32,
    pub conda_env: Option<String>,
    pub run_dir: PathBuf,
    pub priority: u8,
    pub gpu_mem: u64, // in MB
    pub depends_on: Option<u32>,

    /// Optional fields that get populated by gflowd
    pub run_name: Option<String>, // tmux session name
    pub state: JobState,
    pub gpu_ids: Option<Vec<u32>>, // GPU IDs assigned to this job
}

#[derive(Default)]
pub struct JobBuilder {
    script: Option<PathBuf>,
    command: Option<String>,
    gpus: u32,
    conda_env: Option<String>,
    run_dir: PathBuf,
    priority: u8,
    gpu_mem: u64,
    depends_on: Option<u32>,
}

impl JobBuilder {
    pub fn new() -> Self {
        Self {
            priority: 10, // Default priority
            gpu_mem: 0,   // Default no specific memory requirement
            depends_on: None,
            ..Default::default()
        }
    }

    pub fn script(mut self, script: PathBuf) -> Self {
        self.script = Some(script);
        self
    }

    pub fn command(mut self, command: String) -> Self {
        self.command = Some(command);
        self
    }

    pub fn gpus(mut self, gpus: u32) -> Self {
        self.gpus = gpus;
        self
    }

    pub fn conda_env(mut self, conda_env: &Option<String>) -> Self {
        self.conda_env = conda_env.clone();
        self
    }

    pub fn run_dir(mut self, run_dir: PathBuf) -> Self {
        self.run_dir = run_dir;
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn gpu_mem(mut self, gpu_mem: u64) -> Self {
        self.gpu_mem = gpu_mem;
        self
    }

    pub fn depends_on(mut self, depends_on: Option<u32>) -> Self {
        self.depends_on = depends_on;
        self
    }

    pub fn build(self) -> Job {
        Job {
            id: 0,
            script: self.script,
            command: self.command,
            gpus: self.gpus,
            conda_env: self.conda_env,
            priority: self.priority,
            gpu_mem: self.gpu_mem,
            depends_on: self.depends_on,
            run_name: None,
            state: JobState::Queued,
            gpu_ids: None,
            run_dir: self.run_dir,
        }
    }
}

impl Job {
    pub fn builder() -> JobBuilder {
        JobBuilder::new()
    }
}
