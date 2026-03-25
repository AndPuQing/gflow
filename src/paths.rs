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

fn timestamp_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn archived_log_file_path(log_path: &std::path::Path, current_attempt: u32) -> PathBuf {
    let file_name = log_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "job.log".to_string());

    if current_attempt > 0 {
        log_path.with_file_name(format!("{file_name}-retry"))
    } else {
        log_path.with_file_name(format!("{file_name}.old.{}", timestamp_nanos()))
    }
}

fn uniquify_archive_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "job.log.archive".to_string());
    path.with_file_name(format!("{file_name}.dup.{}", timestamp_nanos()))
}

fn prune_retry_log_history_in_dir(
    log_dir: &std::path::Path,
    job_id: u32,
    successful_attempt: u32,
) -> anyhow::Result<()> {
    if successful_attempt == 0 || !log_dir.exists() {
        return Ok(());
    }

    let retained_name = format!("{job_id}.log-retry");
    let previous_managed_prefix = format!("{job_id}.log.attempt-");
    let legacy_prefix = format!("{job_id}.log.old.");

    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();

        if (file_name.starts_with(&previous_managed_prefix)
            || file_name.starts_with(&legacy_prefix))
            && file_name != retained_name
        {
            std::fs::remove_file(entry.path())?;
        }
    }

    Ok(())
}

/// Returns the log file path for a job, archiving any existing log first.
/// Only call this when starting a new job execution to avoid losing active logs.
pub fn prepare_log_file_path(job_id: u32, current_attempt: u32) -> anyhow::Result<PathBuf> {
    let log_path = get_log_file_path(job_id)?;

    if log_path.exists() {
        let archived_path = if current_attempt > 0 {
            let archived_path = archived_log_file_path(&log_path, current_attempt);
            if archived_path.exists() {
                std::fs::remove_file(&archived_path)?;
            }
            archived_path
        } else {
            uniquify_archive_path(archived_log_file_path(&log_path, current_attempt))
        };
        let archived_name = archived_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{job_id}.log.archive"));

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

pub fn prune_retry_log_history(job_id: u32, successful_attempt: u32) -> anyhow::Result<()> {
    let log_dir = get_log_dir()?;
    prune_retry_log_history_in_dir(&log_dir, job_id, successful_attempt)
}

pub fn get_daemon_log_file_path() -> anyhow::Result<PathBuf> {
    Ok(get_log_dir()?.join("daemon.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archived_log_file_path_uses_retry_suffix_for_retries() {
        let log_path = PathBuf::from("/tmp/42.log");
        let archived = archived_log_file_path(&log_path, 2);
        assert_eq!(archived, PathBuf::from("/tmp/42.log-retry"));
    }

    #[test]
    fn archived_log_file_path_uses_timestamp_fallback_for_first_attempt() {
        let log_path = PathBuf::from("/tmp/42.log");
        let archived = archived_log_file_path(&log_path, 0);
        let archived_str = archived.to_string_lossy();
        assert!(archived_str.starts_with("/tmp/42.log.old."));
    }

    #[test]
    fn uniquify_archive_path_adds_suffix_when_target_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let archive_path = temp_dir.path().join("42.log.old.1");
        std::fs::write(&archive_path, "old log").unwrap();

        let unique = uniquify_archive_path(archive_path.clone());

        assert_ne!(unique, archive_path);
        assert!(unique
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("42.log.old.1.dup."));
    }

    #[test]
    fn prune_retry_log_history_keeps_retry_log_on_success() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_dir = temp_dir.path();

        std::fs::write(log_dir.join("42.log-retry"), "last failure").unwrap();
        std::fs::write(log_dir.join("42.log.old.123"), "legacy").unwrap();
        std::fs::write(log_dir.join("99.log-retry"), "other job").unwrap();

        prune_retry_log_history_in_dir(log_dir, 42, 2).unwrap();

        assert!(log_dir.join("42.log-retry").exists());
        assert!(!log_dir.join("42.log.old.123").exists());
        assert!(log_dir.join("99.log-retry").exists());
    }
}
