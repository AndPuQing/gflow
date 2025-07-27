use crate::cli;
use anyhow::{Context, Result};

pub(crate) async fn handle_logs(logs_args: cli::LogsArgs) -> Result<()> {
    let log_file = gflow::core::get_log_file_path(logs_args.id)?;
    if !log_file.exists() {
        anyhow::bail!("Log file not found for job {}", logs_args.id);
    }

    let content = std::fs::read_to_string(&log_file)
        .with_context(|| format!("Failed to read log file for job {}", logs_args.id))?;

    println!("{content}");

    Ok(())
}
