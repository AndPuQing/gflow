use anyhow::{anyhow, Context, Result};
use gflow::client::Client;
use gflow::core::job::Job;
use std::path::PathBuf;
use std::time::Duration;

#[allow(clippy::too_many_arguments)]
pub async fn handle_redo(
    config_path: &Option<PathBuf>,
    job_id_str: &str,
    gpus_override: Option<u32>,
    priority_override: Option<u8>,
    depends_on_override: Option<String>,
    time_override: Option<String>,
    conda_env_override: Option<String>,
    clear_deps: bool,
) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    // Resolve job ID (handle @ shorthand)
    let job_id = resolve_job_id(&client, job_id_str).await?;

    // Retrieve the original job
    let original_job = match client.get_job(job_id).await? {
        Some(job) => job,
        None => {
            return Err(anyhow!("Job {} not found.", job_id));
        }
    };

    println!("Resubmitting job {} with parameters:", original_job.id);

    // Build new job based on original
    let mut builder = Job::builder();

    // Preserve core job parameters
    if let Some(ref script) = original_job.script {
        builder = builder.script(script.clone());
        println!("  Script:       {}", script.display());
    }
    if let Some(ref command) = original_job.command {
        builder = builder.command(command.clone());
        println!("  Command:      {}", command);
    }

    // Apply GPUs (override or original)
    let gpus = gpus_override.unwrap_or(original_job.gpus);
    builder = builder.gpus(gpus);
    println!("  GPUs:         {}", gpus);

    // Apply priority (override or original)
    let priority = priority_override.unwrap_or(original_job.priority);
    builder = builder.priority(priority);
    println!("  Priority:     {}", priority);

    // Apply conda environment (override or original)
    let conda_env = if let Some(ref override_env) = conda_env_override {
        Some(override_env.clone())
    } else {
        original_job.conda_env.clone()
    };
    builder = builder.conda_env(&conda_env);
    if let Some(ref env) = conda_env {
        println!("  Conda env:    {}", env);
    }

    // Apply time limit (override or original)
    let time_limit = if let Some(ref time_str) = time_override {
        Some(parse_time_limit(time_str)?)
    } else {
        original_job.time_limit
    };
    builder = builder.time_limit(time_limit);
    if let Some(limit) = time_limit {
        println!("  Time limit:   {}", format_duration(limit));
    }

    // Handle dependency
    let depends_on = if clear_deps {
        println!("  Dependencies: (cleared)");
        None
    } else if let Some(ref dep_str) = depends_on_override {
        let resolved_dep = resolve_dependency(&client, dep_str).await?;
        println!("  Depends on:   {}", resolved_dep);
        Some(resolved_dep)
    } else {
        if let Some(dep) = original_job.depends_on {
            println!("  Depends on:   {}", dep);
        }
        original_job.depends_on
    };
    builder = builder.depends_on(depends_on);

    // Preserve other parameters
    builder = builder.run_dir(original_job.run_dir.clone());
    builder = builder.task_id(original_job.task_id);

    // Set the submitter to current user
    let username = gflow::core::get_current_username();
    builder = builder.submitted_by(username);

    // Build and submit the job
    let new_job = builder.build();
    let response = client
        .add_job(new_job)
        .await
        .context("Failed to submit job")?;

    println!(
        "\nSubmitted batch job {} ({})",
        response.id, response.run_name
    );

    Ok(())
}

/// Resolve job ID from string (handles @ shorthand notation)
async fn resolve_job_id(client: &Client, job_id_str: &str) -> Result<u32> {
    let trimmed = job_id_str.trim();

    if trimmed.starts_with('@') {
        // Use dependency resolution to handle @ shorthand
        let username = gflow::core::get_current_username();
        client
            .resolve_dependency(&username, trimmed)
            .await
            .with_context(|| format!("Failed to resolve job ID '{}'", trimmed))
    } else {
        // Parse as numeric job ID
        trimmed
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid job ID: {}", trimmed))
    }
}

/// Resolve dependency expression to job ID
async fn resolve_dependency(client: &Client, depends_on: &str) -> Result<u32> {
    let trimmed = depends_on.trim();

    if trimmed.starts_with('@') {
        let username = gflow::core::get_current_username();
        client
            .resolve_dependency(&username, trimmed)
            .await
            .with_context(|| format!("Failed to resolve dependency '{}'", trimmed))
    } else {
        trimmed
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid dependency value: {}", trimmed))
    }
}

/// Parse time limit string into Duration
/// Supported formats:
/// - "HH:MM:SS" - hours:minutes:seconds
/// - "MM:SS" - minutes:seconds
/// - "MM" - minutes
fn parse_time_limit(time_str: &str) -> Result<Duration> {
    let parts: Vec<&str> = time_str.split(':').collect();

    match parts.len() {
        1 => {
            // Minutes as a single number
            let val = time_str
                .parse::<u64>()
                .context("Invalid time format. Expected number of minutes")?;
            Ok(Duration::from_secs(val * 60))
        }
        2 => {
            // MM:SS
            let minutes = parts[0]
                .parse::<u64>()
                .context("Invalid minutes in MM:SS format")?;
            let seconds = parts[1]
                .parse::<u64>()
                .context("Invalid seconds in MM:SS format")?;
            Ok(Duration::from_secs(minutes * 60 + seconds))
        }
        3 => {
            // HH:MM:SS
            let hours = parts[0]
                .parse::<u64>()
                .context("Invalid hours in HH:MM:SS format")?;
            let minutes = parts[1]
                .parse::<u64>()
                .context("Invalid minutes in HH:MM:SS format")?;
            let seconds = parts[2]
                .parse::<u64>()
                .context("Invalid seconds in HH:MM:SS format")?;
            Ok(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
        }
        _ => Err(anyhow!(
            "Invalid time format. Expected formats: HH:MM:SS, MM:SS, or MM"
        )),
    }
}

/// Format duration for display
fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
