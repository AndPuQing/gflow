use std::path::PathBuf;

/// Honor the XDG base directory environment variables on every platform.
///
/// The `dirs` crate only consults them on Linux; on macOS it always returns
/// `~/Library/...` paths. Explicitly preferring `XDG_*` when set keeps gflow's
/// config/data/runtime locations deterministic across platforms (e.g. in
/// tests or container-style environments) without changing defaults for users
/// who don't set them.
fn xdg_dir(env_key: &str) -> Option<PathBuf> {
    let value = std::env::var_os(env_key)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

pub fn get_config_dir() -> anyhow::Result<PathBuf> {
    xdg_dir("XDG_CONFIG_HOME")
        .or_else(dirs::config_dir)
        .ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))
        .map(|p| p.join("gflow"))
}

pub fn get_data_dir() -> anyhow::Result<PathBuf> {
    xdg_dir("XDG_DATA_HOME")
        .or_else(dirs::data_dir)
        .ok_or_else(|| anyhow::anyhow!("Failed to get data directory"))
        .map(|p| p.join("gflow"))
}

pub fn get_runtime_dir() -> anyhow::Result<PathBuf> {
    xdg_dir("XDG_RUNTIME_DIR")
        .or_else(dirs::runtime_dir)
        .or_else(dirs::cache_dir)
        .ok_or_else(|| anyhow::anyhow!("Failed to get runtime or cache directory"))
        .map(|p| p.join("gflow"))
}

fn get_log_dir() -> anyhow::Result<PathBuf> {
    Ok(get_data_dir()?.join("logs"))
}

/// Returns the log file path for a job without any side effects.
pub fn get_log_file_path(job_id: u32) -> anyhow::Result<PathBuf> {
    Ok(get_log_dir()?.join(format!("{job_id}.log")))
}

/// Returns the log file path for a job, archiving any existing log first.
/// Only call this when starting a new job execution to avoid losing active logs.
pub fn prepare_log_file_path(job_id: u32) -> anyhow::Result<PathBuf> {
    let log_path = get_log_file_path(job_id)?;

    if log_path.exists() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let archived_name = format!("{job_id}.log.old.{timestamp}");
        let archived_path = log_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(&archived_name);

        if let Err(error) = std::fs::rename(&log_path, &archived_path) {
            tracing::warn!(
                "Failed to archive existing log {:?} to {:?}: {}",
                log_path,
                archived_path,
                error
            );
        } else {
            tracing::info!(
                "Archived existing log for job {} to {}",
                job_id,
                archived_name
            );
        }
    }

    Ok(log_path)
}

pub fn get_daemon_log_file_path() -> anyhow::Result<PathBuf> {
    Ok(get_log_dir()?.join("daemon.log"))
}

/// Directory containing durable execution metadata for process-backed jobs.
///
/// This is deliberately separate from scheduler state: the runner remains a
/// valid source of truth while the daemon is offline.
pub fn get_runner_dir() -> anyhow::Result<PathBuf> {
    Ok(get_data_dir()?.join("runners"))
}

pub fn get_runner_metadata_path(job_id: u32) -> anyhow::Result<PathBuf> {
    Ok(get_runner_dir()?.join(format!("{job_id}.json")))
}

pub fn get_runner_result_path(job_id: u32) -> anyhow::Result<PathBuf> {
    Ok(get_runner_dir()?.join(format!("{job_id}.result.json")))
}
