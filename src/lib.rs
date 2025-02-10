use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub enum JobState {
    Queued,
    Running,
    Finished,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Job {
    /// Required fields at submission time
    pub script: PathBuf,
    pub gpus: u32,

    /// Optional fields that get populated by gflowd
    pub run_name: Option<String>, // tmux session name
    pub state: JobState,
    pub gpu_ids: Option<Vec<u32>>, // GPU IDs assigned to this job
}

impl Job {
    pub fn new(script: PathBuf, gpus: u32) -> Self {
        Self {
            script,
            gpus,
            run_name: None,
            state: JobState::Queued,
            gpu_ids: None,
        }
    }
}

pub trait GPU {
    fn get_gpu_count() -> u32;
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
}
