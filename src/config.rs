use crate::core::gpu_allocation::GpuAllocationStrategy;
use crate::paths::get_config_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    /// Timezone for displaying and parsing times (e.g., "Asia/Shanghai", "America/Los_Angeles", "UTC")
    /// If not set, uses local timezone
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Webhook/notification settings for gflowd
    #[serde(default)]
    #[serde(skip_serializing_if = "NotificationsConfig::is_default")]
    pub notifications: NotificationsConfig,
    /// Project tracking settings
    #[serde(default)]
    #[serde(skip_serializing_if = "ProjectsConfig::is_default")]
    pub projects: ProjectsConfig,
    /// Per-user / per-project resource quotas
    #[serde(default)]
    #[serde(skip_serializing_if = "QuotaConfig::is_empty")]
    pub quota: QuotaConfig,
    /// Job executor configuration
    #[serde(default)]
    #[serde(skip_serializing_if = "ExecutorConfig::is_default")]
    pub executor: ExecutorConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DaemonConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Limit which GPUs the scheduler can use (None = all GPUs)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpus: Option<Vec<u32>>,
    /// GPU assignment strategy when selecting from available GPUs.
    #[serde(default)]
    pub gpu_allocation_strategy: GpuAllocationStrategy,
    /// How often to poll NVML for GPU occupancy updates.
    #[serde(default = "default_gpu_poll_interval_secs")]
    #[serde(skip_serializing_if = "is_default_gpu_poll_interval_secs")]
    pub gpu_poll_interval_secs: u64,
    /// Fair-share scheduling settings.
    #[serde(default)]
    #[serde(skip_serializing_if = "FairShareConfig::is_default")]
    pub fair_share: FairShareConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NotificationsConfig {
    /// Enable notification system (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// List of webhook endpoints
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub webhooks: Vec<WebhookConfig>,
    /// List of email endpoints
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<EmailConfig>,
    /// Limit concurrent notification deliveries across all endpoints
    #[serde(default = "default_max_concurrent_deliveries")]
    #[serde(skip_serializing_if = "is_default_max_concurrent_deliveries")]
    pub max_concurrent_deliveries: usize,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            webhooks: vec![],
            emails: vec![],
            max_concurrent_deliveries: default_max_concurrent_deliveries(),
        }
    }
}

impl NotificationsConfig {
    fn is_default(value: &Self) -> bool {
        !value.enabled
            && value.webhooks.is_empty()
            && value.emails.is_empty()
            && value.max_concurrent_deliveries == default_max_concurrent_deliveries()
    }
}

fn default_max_concurrent_deliveries() -> usize {
    16
}

fn is_default_max_concurrent_deliveries(v: &usize) -> bool {
    *v == default_max_concurrent_deliveries()
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct ProjectsConfig {
    /// List of known/allowed project codes
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub known_projects: Vec<String>,
    /// Require project to be specified for all jobs
    #[serde(default)]
    pub require_project: bool,
}

impl ProjectsConfig {
    fn is_default(value: &Self) -> bool {
        value.known_projects.is_empty() && !value.require_project
    }
}

/// Resource limits for a single quota subject (a user, a project, or one of
/// the two defaults). `None` fields mean "unlimited"; when both a default and
/// a named entry apply, fields are merged with the named entry winning.
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct QuotaLimits {
    /// Maximum number of concurrently running jobs.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_running_jobs: Option<usize>,
    /// Maximum number of concurrently allocated GPUs.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_running_gpus: Option<u32>,
    /// Maximum number of pending (queued/held) jobs; enforced at submission.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_queued_jobs: Option<usize>,
}

impl QuotaLimits {
    pub fn is_empty(&self) -> bool {
        self.max_running_jobs.is_none()
            && self.max_running_gpus.is_none()
            && self.max_queued_jobs.is_none()
    }

    /// Field-wise merge: `Some` fields in `other` override `self`.
    pub fn merged_with(&self, other: &QuotaLimits) -> QuotaLimits {
        QuotaLimits {
            max_running_jobs: other.max_running_jobs.or(self.max_running_jobs),
            max_running_gpus: other.max_running_gpus.or(self.max_running_gpus),
            max_queued_jobs: other.max_queued_jobs.or(self.max_queued_jobs),
        }
    }
}

/// Per-user / per-project resource quotas.
///
/// Configured via the `[quota]` section in `gflow.toml`. Runtime overrides set
/// through `gctl quota` are persisted in the daemon state and take precedence
/// over the file-based baseline (merged field-wise).
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq)]
pub struct QuotaConfig {
    /// Limits applied to every user, unless overridden in `users`.
    #[serde(default)]
    #[serde(skip_serializing_if = "QuotaLimits::is_empty")]
    pub default_user: QuotaLimits,
    /// Limits applied to every project, unless overridden in `projects`.
    #[serde(default)]
    #[serde(skip_serializing_if = "QuotaLimits::is_empty")]
    pub default_project: QuotaLimits,
    /// Per-user limits, keyed by username.
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub users: HashMap<String, QuotaLimits>,
    /// Per-project limits, keyed by project code.
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub projects: HashMap<String, QuotaLimits>,
}

impl QuotaConfig {
    pub fn is_empty(&self) -> bool {
        self.default_user.is_empty()
            && self.default_project.is_empty()
            && self.users.is_empty()
            && self.projects.is_empty()
    }

    /// Field-wise merge: entries/fields set in `other` override `self`.
    pub fn merged_with(&self, other: &QuotaConfig) -> QuotaConfig {
        let mut users = self.users.clone();
        for (name, limits) in &other.users {
            users
                .entry(name.clone())
                .and_modify(|existing| *existing = existing.merged_with(limits))
                .or_insert_with(|| limits.clone());
        }
        let mut projects = self.projects.clone();
        for (name, limits) in &other.projects {
            projects
                .entry(name.clone())
                .and_modify(|existing| *existing = existing.merged_with(limits))
                .or_insert_with(|| limits.clone());
        }
        QuotaConfig {
            default_user: self.default_user.merged_with(&other.default_user),
            default_project: self.default_project.merged_with(&other.default_project),
            users,
            projects,
        }
    }

    /// Effective limits for a user: `default_user` merged with `users[user]`.
    pub fn user_limits(&self, user: &str) -> QuotaLimits {
        match self.users.get(user) {
            Some(limits) => self.default_user.merged_with(limits),
            None => self.default_user.clone(),
        }
    }

    /// Effective limits for a project: `default_project` merged with
    /// `projects[project]`. Jobs without a project have no project quota.
    pub fn project_limits(&self, project: Option<&str>) -> Option<QuotaLimits> {
        let project = project?;
        Some(match self.projects.get(project) {
            Some(limits) => self.default_project.merged_with(limits),
            None => self.default_project.clone(),
        })
    }
}

/// Job execution backend.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecutorType {
    /// Spawn a detached child process group (setsid) with stdio redirected to
    /// the job log file. The default; requires no tmux.
    #[default]
    Process,
    /// Legacy tmux session + terminal key injection. Enables `gjob attach`
    /// and `gjob close-sessions` for running jobs.
    Tmux,
}

/// Job executor settings, configured via the `[executor]` table in `gflow.toml`.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ExecutorConfig {
    /// Which executor to use for job execution: "process" (default) or "tmux".
    #[serde(default)]
    pub r#type: ExecutorType,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            r#type: ExecutorType::Process,
        }
    }
}

impl ExecutorConfig {
    fn is_default(value: &Self) -> bool {
        value.r#type == ExecutorType::Process
    }
}

/// Fair-share scheduling: reorder jobs within the same priority band so that
/// users who have consumed less GPU-time recently are scheduled first.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FairShareConfig {
    /// Whether fair-share reordering influences scheduling (default: true).
    /// Accounting of historical usage happens regardless of this flag.
    #[serde(default = "default_fair_share_enabled")]
    pub enabled: bool,
    /// Half-life (in seconds) for the exponential decay of historical GPU-time
    /// usage (default: 7 days). Smaller values make scheduling react faster to
    /// recent usage; larger values smooth fairness over a longer window.
    #[serde(default = "default_fair_share_half_life_secs")]
    #[serde(skip_serializing_if = "is_default_fair_share_half_life_secs")]
    pub half_life_secs: u64,
}

impl Default for FairShareConfig {
    fn default() -> Self {
        Self {
            enabled: default_fair_share_enabled(),
            half_life_secs: default_fair_share_half_life_secs(),
        }
    }
}

impl FairShareConfig {
    fn is_default(value: &Self) -> bool {
        value.enabled == default_fair_share_enabled()
            && value.half_life_secs == default_fair_share_half_life_secs()
    }
}

fn default_fair_share_enabled() -> bool {
    true
}

fn default_fair_share_half_life_secs() -> u64 {
    7 * 24 * 3600
}

fn is_default_fair_share_half_life_secs(v: &u64) -> bool {
    *v == default_fair_share_half_life_secs()
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct WebhookConfig {
    pub url: String,
    /// Events to subscribe to. Supports `"*"` (all).
    ///
    /// Examples: `["job_completed", "job_failed"]`, `["*"]`
    #[serde(default = "default_webhook_events")]
    pub events: Vec<String>,
    /// Optional: only notify for specific users (job submitter / reservation owner)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_users: Option<Vec<String>>,
    /// Optional: custom HTTP headers (e.g., Authorization)
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// Optional: per-delivery timeout in seconds (default: 10)
    #[serde(default = "default_webhook_timeout_secs")]
    pub timeout_secs: u64,
    /// Optional: number of retries after the initial attempt (default: 3)
    #[serde(default = "default_webhook_max_retries")]
    pub max_retries: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EmailConfig {
    /// SMTP connection URL (e.g. "smtps://user:pass@smtp.example.com:465")
    pub smtp_url: String,
    /// From mailbox (supports display name syntax like "gflow <noreply@example.com>")
    pub from: String,
    /// Recipient mailboxes
    #[serde(default)]
    pub to: Vec<String>,
    /// Events to subscribe to. Supports `"*"` (all).
    #[serde(default = "default_email_events")]
    pub events: Vec<String>,
    /// Optional: only notify for specific users (job submitter / reservation owner)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_users: Option<Vec<String>>,
    /// Optional subject prefix, e.g. "[gflow-prod]"
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_prefix: Option<String>,
    /// Optional: per-delivery timeout in seconds (default: 10)
    #[serde(default = "default_email_timeout_secs")]
    pub timeout_secs: u64,
    /// Optional: number of retries after the initial attempt (default: 3)
    #[serde(default = "default_email_max_retries")]
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

fn default_email_events() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_email_timeout_secs() -> u64 {
    10
}

fn default_email_max_retries() -> u32 {
    3
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    59000
}

fn default_gpu_poll_interval_secs() -> u64 {
    10
}

fn is_default_gpu_poll_interval_secs(v: &u64) -> bool {
    *v == default_gpu_poll_interval_secs()
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            gpus: None,
            gpu_allocation_strategy: GpuAllocationStrategy::default(),
            gpu_poll_interval_secs: default_gpu_poll_interval_secs(),
            fair_share: FairShareConfig::default(),
        }
    }
}

#[test]
fn quota_config_merges_fields_and_entries() {
    let baseline = QuotaConfig {
        default_user: QuotaLimits {
            max_running_jobs: Some(4),
            max_running_gpus: Some(2),
            max_queued_jobs: None,
        },
        users: [(
            "alice".to_string(),
            QuotaLimits {
                max_running_gpus: Some(4),
                ..Default::default()
            },
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let overrides = QuotaConfig {
        default_user: QuotaLimits {
            max_queued_jobs: Some(50),
            ..Default::default()
        },
        users: [
            (
                "alice".to_string(),
                QuotaLimits {
                    max_running_jobs: Some(1),
                    ..Default::default()
                },
            ),
            (
                "bob".to_string(),
                QuotaLimits {
                    max_running_gpus: Some(8),
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };

    let merged = baseline.merged_with(&overrides);

    // default_user: override adds max_queued_jobs, keeps baseline fields.
    assert_eq!(
        merged.default_user,
        QuotaLimits {
            max_running_jobs: Some(4),
            max_running_gpus: Some(2),
            max_queued_jobs: Some(50),
        }
    );
    // alice: baseline entry merged with override entry.
    assert_eq!(
        merged.user_limits("alice"),
        QuotaLimits {
            max_running_jobs: Some(1),
            max_running_gpus: Some(4),
            max_queued_jobs: Some(50),
        }
    );
    // bob: override-only entry plus merged defaults.
    assert_eq!(
        merged.user_limits("bob"),
        QuotaLimits {
            max_running_jobs: Some(4),
            max_running_gpus: Some(8),
            max_queued_jobs: Some(50),
        }
    );
    // Unknown user falls back to merged defaults.
    assert_eq!(merged.user_limits("carol").max_running_gpus, Some(2));
}

pub fn load_config(config_path: Option<&PathBuf>) -> Result<Config, config::ConfigError> {
    let mut config_vec = vec![];

    // Default config file
    if let Ok(default_config_path) = get_config_dir().map(|d| d.join("gflow.toml")) {
        if default_config_path.exists() {
            config_vec.push(default_config_path);
        }
    }

    // User-provided config file (should override defaults)
    if let Some(config_path) = config_path {
        if config_path.exists() {
            config_vec.push(config_path.clone());
        } else {
            eprintln!("Warning: Config file {config_path:?} not found.");
        }
    }

    let settings = config::Config::builder();
    let settings = config_vec.iter().fold(settings, |s, path| {
        s.add_source(config::File::from(path.as_path()))
    });

    settings
        .add_source(environment_source(None))
        .build()?
        .try_deserialize()
}

fn environment_source(source: Option<config::Map<String, String>>) -> config::Environment {
    config::Environment::with_prefix("GFLOW")
        .prefix_separator("_")
        .separator("__")
        .source(source)
        .try_parsing(true)
        .list_separator(",")
        .with_list_parse_key("daemon.gpus")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_source_applies_gpu_allocation_strategy() {
        let mut env = config::Map::new();
        env.insert(
            "GFLOW_DAEMON__GPU_ALLOCATION_STRATEGY".to_string(),
            "random".to_string(),
        );

        let config = config::Config::builder()
            .add_source(environment_source(Some(env)))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap();

        assert_eq!(
            config.daemon.gpu_allocation_strategy,
            GpuAllocationStrategy::Random
        );
    }

    #[test]
    fn environment_source_applies_gpu_poll_interval() {
        let mut env = config::Map::new();
        env.insert(
            "GFLOW_DAEMON__GPU_POLL_INTERVAL_SECS".to_string(),
            "3".to_string(),
        );

        let config = config::Config::builder()
            .add_source(environment_source(Some(env)))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap();

        assert_eq!(config.daemon.gpu_poll_interval_secs, 3);
    }

    #[test]
    fn fair_share_defaults_are_sane() {
        let fs = FairShareConfig::default();
        assert!(fs.enabled);
        assert_eq!(fs.half_life_secs, 7 * 24 * 3600);
    }

    #[test]
    fn environment_source_applies_fair_share_settings() {
        let mut env = config::Map::new();
        env.insert(
            "GFLOW_DAEMON__FAIR_SHARE__ENABLED".to_string(),
            "false".to_string(),
        );
        env.insert(
            "GFLOW_DAEMON__FAIR_SHARE__HALF_LIFE_SECS".to_string(),
            "3600".to_string(),
        );

        let config = config::Config::builder()
            .add_source(environment_source(Some(env)))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap();

        assert!(!config.daemon.fair_share.enabled);
        assert_eq!(config.daemon.fair_share.half_life_secs, 3600);
    }

    #[test]
    fn quota_toml_section_parses() {
        let config = config::Config::builder()
            .add_source(config::File::from_str(
                r#"
[quota]
default_user = { max_running_jobs = 4, max_running_gpus = 2, max_queued_jobs = 50 }
default_project = { max_running_gpus = 6 }

[quota.users]
alice = { max_running_gpus = 4 }

[quota.projects]
cv-team = { max_running_gpus = 8, max_queued_jobs = 100 }
"#,
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap();

        assert_eq!(config.quota.default_user.max_running_jobs, Some(4));
        assert_eq!(config.quota.default_project.max_running_gpus, Some(6));
        assert_eq!(
            config.quota.user_limits("alice"),
            QuotaLimits {
                max_running_jobs: Some(4),
                max_running_gpus: Some(4),
                max_queued_jobs: Some(50),
            }
        );
        assert_eq!(
            config.quota.project_limits(Some("cv-team")),
            Some(QuotaLimits {
                max_running_jobs: None,
                max_running_gpus: Some(8),
                max_queued_jobs: Some(100),
            })
        );
        // Jobs without a project have no project quota.
        assert_eq!(config.quota.project_limits(None), None);
    }

    #[test]
    fn environment_source_rejects_invalid_gpu_poll_interval() {
        let mut env = config::Map::new();
        env.insert(
            "GFLOW_DAEMON__GPU_POLL_INTERVAL_SECS".to_string(),
            "abc".to_string(),
        );

        let error = config::Config::builder()
            .add_source(environment_source(Some(env)))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap_err();

        assert!(error.to_string().contains("invalid type"));
    }

    #[test]
    fn executor_defaults_to_process() {
        assert_eq!(ExecutorConfig::default().r#type, ExecutorType::Process);
    }

    #[test]
    fn executor_toml_section_parses_process_and_tmux() {
        let config = config::Config::builder()
            .add_source(config::File::from_str(
                r#"
[executor]
type = "tmux"
"#,
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap();
        assert_eq!(config.executor.r#type, ExecutorType::Tmux);

        let config = config::Config::builder()
            .add_source(config::File::from_str(
                r#"
[executor]
type = "process"
"#,
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap();
        assert_eq!(config.executor.r#type, ExecutorType::Process);
    }

    #[test]
    fn environment_source_does_not_treat_single_underscore_as_nested_separator() {
        let mut env = config::Map::new();
        env.insert(
            "GFLOW_DAEMON_GPU_POLL_INTERVAL_SECS".to_string(),
            "3".to_string(),
        );

        let config = config::Config::builder()
            .add_source(environment_source(Some(env)))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap();

        assert_eq!(config.daemon.gpu_poll_interval_secs, 10);
    }
}
