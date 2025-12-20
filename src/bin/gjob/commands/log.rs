use anyhow::{Context, Result};
use gflow::client::Client;
use std::io::{self, Write};
use std::path::PathBuf;

pub async fn handle_log(config_path: &Option<PathBuf>, job_id_str: &str) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    // Resolve job ID (handle @ shorthand)
    let job_id = crate::utils::resolve_job_id(&client, job_id_str).await?;

    let log_path = match client.get_job_log_path(job_id).await? {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("Log for job {} is not available.", job_id);
            return Ok(());
        }
    };

    let mut file = std::fs::File::open(&log_path).with_context(|| {
        format!(
            "Failed to open log file '{}' for job {}",
            log_path.display(),
            job_id
        )
    })?;

    let mut stdout = io::stdout();
    io::copy(&mut file, &mut stdout).context("Failed to write log contents to stdout")?;
    stdout.flush().context("Failed to flush stdout")?;

    Ok(())
}
