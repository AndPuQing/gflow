pub mod job;
use std::{collections::HashMap, path::PathBuf};
pub type UUID = String;

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
    use crate::job::Job;

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
