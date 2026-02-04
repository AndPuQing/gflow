use crate::core::get_config_dir;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize, Debug, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    /// Timezone for displaying and parsing times (e.g., "Asia/Shanghai", "America/Los_Angeles", "UTC")
    /// If not set, uses local timezone
    #[serde(default)]
    pub timezone: Option<String>,
    /// Webhook/notification settings for gflowd
    #[serde(default)]
    pub notifications: NotificationsConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DaemonConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Limit which GPUs the scheduler can use (None = all GPUs)
    #[serde(default)]
    pub gpus: Option<Vec<u32>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct NotificationsConfig {
    /// Enable notification system (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// List of webhook endpoints
    #[serde(default)]
    pub webhooks: Vec<WebhookConfig>,
    /// Limit concurrent webhook deliveries across all endpoints
    #[serde(default = "default_max_concurrent_deliveries")]
    pub max_concurrent_deliveries: usize,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            webhooks: vec![],
            max_concurrent_deliveries: default_max_concurrent_deliveries(),
        }
    }
}

fn default_max_concurrent_deliveries() -> usize {
    16
}

#[derive(Deserialize, Debug, Clone)]
pub struct WebhookConfig {
    pub url: String,
    /// Events to subscribe to. Supports `"*"` (all).
    ///
    /// Examples: `["job_completed", "job_failed"]`, `["*"]`
    #[serde(default = "default_webhook_events")]
    pub events: Vec<String>,
    /// Optional: only notify for specific users (job submitter / reservation owner)
    #[serde(default)]
    pub filter_users: Option<Vec<String>>,
    /// Optional: custom HTTP headers (e.g., Authorization)
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Optional: per-delivery timeout in seconds (default: 10)
    #[serde(default = "default_webhook_timeout_secs")]
    pub timeout_secs: u64,
    /// Optional: number of retries after the initial attempt (default: 3)
    #[serde(default = "default_webhook_max_retries")]
    pub max_retries: u32,
}

fn default_webhook_events() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_webhook_timeout_secs() -> u64 {
    10
}

fn default_webhook_max_retries() -> u32 {
    3
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
        .add_source(
            config::Environment::with_prefix("GFLOW")
                .separator("_")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("daemon.gpus"),
        )
        .build()?
        .try_deserialize()
}
