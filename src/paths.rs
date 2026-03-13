use std::path::PathBuf;

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
        let archived_path = log_path.parent().unwrap().join(&archived_name);

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
