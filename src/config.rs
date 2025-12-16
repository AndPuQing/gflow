use crate::core::get_config_dir;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
}

#[derive(Deserialize, Debug)]
pub struct DaemonConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Limit which GPUs the scheduler can use (None = all GPUs)
    #[serde(default)]
    pub gpus: Option<Vec<u32>>,
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    59000
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            gpus: None,
        }
    }
}

pub fn load_config(config_path: Option<&PathBuf>) -> Result<Config, config::ConfigError> {
    let mut config_vec = vec![];

    // User-provided config file
    if let Some(config_path) = config_path {
        if config_path.exists() {
            config_vec.push(config_path.clone());
        } else {
            eprintln!("Warning: Config file {config_path:?} not found.");
        }
    }

    // Default config file
    if let Ok(default_config_path) = get_config_dir().map(|d| d.join("gflow.toml")) {
        if default_config_path.exists() {
            config_vec.push(default_config_path);
        }
    }

    let settings = config::Config::builder();
    let settings = config_vec.iter().fold(settings, |s, path| {
        s.add_source(config::File::from(path.as_path()))
    });

    settings
        .add_source(config::Environment::with_prefix("GFLOW"))
        .build()?
        .try_deserialize()
}
