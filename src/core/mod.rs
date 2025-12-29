pub mod executor;
pub mod info;
pub mod job;
pub mod migrations;
pub mod scheduler;

use std::{collections::HashMap, env, path::PathBuf};
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
    use std::sync::OnceLock;
    static VERSION: OnceLock<String> = OnceLock::new();

    VERSION.get_or_init(|| {
        let author = clap::crate_authors!();
        format!(
            "\
{VERSION_MESSAGE}
Authors: {author}"
        )
    })
}

#[derive(Debug, Clone)]
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

pub fn get_current_username() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
