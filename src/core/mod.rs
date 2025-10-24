pub mod executor;
pub mod info;
pub mod job;

use rand::Rng;
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

use nvml_wrapper::Nvml;

pub trait GPU {
    fn get_gpus(nvml: &Nvml) -> HashMap<UUID, GPUSlot>;
}

pub fn get_config_dir() -> anyhow::Result<PathBuf> {
    dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))
        .map(|p| p.join("gflow"))
}

pub fn get_data_dir() -> anyhow::Result<PathBuf> {
    dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get data directory"))
        .map(|p| p.join("gflow"))
}

pub fn get_runtime_dir() -> anyhow::Result<PathBuf> {
    dirs::runtime_dir()
        .or_else(dirs::cache_dir)
        .ok_or_else(|| anyhow::anyhow!("Failed to get runtime or cache directory"))
        .map(|p| p.join("gflow"))
}

pub fn get_log_file_path(job_id: u32) -> anyhow::Result<PathBuf> {
    let log_dir = get_data_dir()?.join("logs");
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir)?;
    }
    Ok(log_dir.join(format!("{job_id}.log")))
}

pub fn random_run_name() -> String {
    const WORDS: &[&str] = &[
        "Lion", "Tiger", "Elephant", "Giraffe", "Bear", "Monkey", "Zebra", "Kangaroo", "Panda",
        "Penguin", "Happy", "Sad", "Angry", "Sleepy", "Hungry", "Thirsty", "Silly", "Crazy",
        "Funny", "Grumpy",
    ];

    let mut rng = rand::rng();
    format!(
        "{}-{}",
        WORDS[rng.random_range(0..WORDS.len())].to_lowercase(),
        &uuid::Uuid::new_v4().to_string()[..8]
    )
}
