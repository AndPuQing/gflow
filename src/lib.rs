use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

pub type UUID = String;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub enum JobState {
    Queued,
    Running,
    Finished,
    Failed,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Job {
    /// Required fields at submission time
    pub script: Option<PathBuf>,
    pub command: Option<String>,
    pub gpus: u32,
    pub conda_env: Option<String>,

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
}

impl JobBuilder {
    pub fn new() -> Self {
        Self::default()
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

    pub fn build(self) -> Job {
        Job {
            script: self.script,
            command: self.command,
            gpus: self.gpus,
            conda_env: self.conda_env,
            run_name: None,
            state: JobState::Queued,
            gpu_ids: None,
        }
    }
}

impl Job {
    pub fn builder() -> JobBuilder {
        JobBuilder::new()
    }
}

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("VERGEN_BUILD_TIMESTAMP"),
    ")\n",
    "Branch: ",
    env!("VERGEN_GIT_BRANCH"),
    "\nCommit: ",
    env!("VERGEN_GIT_SHA"),
);

pub fn version() -> &'static str {
    let author = clap::crate_authors!();

    Box::leak(Box::new(format!(
        "\
{VERSION_MESSAGE}
Authors: {author}"
    )))
}

#[derive(Debug)]
pub struct GPUSlot {
    pub index: u32,
    pub available: bool,
}

pub trait GPU {
    fn get_gpus() -> HashMap<UUID, GPUSlot>;
}

pub fn get_config_temp_dir() -> PathBuf {
    dirs::config_dir().unwrap().join("gflowd")
}

pub fn get_config_temp_file() -> PathBuf {
    get_config_temp_dir().join("gflowdrc")
}

pub fn random_run_name() -> String {
    let words = vec![
        "Lion", "Tiger", "Elephant", "Giraffe", "Bear", "Monkey", "Zebra", "Kangaroo", "Panda",
        "Penguin", "Happy", "Sad", "Angry", "Sleepy", "Hungry", "Thirsty", "Silly", "Crazy",
        "Funny", "Grumpy",
    ];

    use rand::Rng;
    let mut rng = rand::rng();
    let word = words[rng.random_range(0..words.len())].to_lowercase();
    let number = rng.random_range(0..10);
    format!("{}-{}", word, number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_run_name() {
        let name = random_run_name();
        assert!(name.contains("-"));
    }

    #[test]
    fn test_job_builder() {
        let job = Job::builder()
            .script(PathBuf::from("test.sh"))
            .gpus(1)
            .build();

        assert_eq!(job.script, Some(PathBuf::from("test.sh")));
        assert_eq!(job.gpus, 1);
    }
}
