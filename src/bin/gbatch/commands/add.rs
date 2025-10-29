use crate::cli;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use gflow::client::Client;
use gflow::core::job::Job;
use std::{env, fs, path::PathBuf, time::Duration};

pub(crate) async fn handle_add(
    config: &gflow::config::Config,
    add_args: cli::AddArgs,
) -> Result<()> {
    let client = Client::build(config).context("Failed to build client")?;

    if let Some(array_spec) = &add_args.array {
        let task_ids = parse_array_spec(array_spec)?;
        let mut job_ids = Vec::new();
        for task_id in task_ids {
            let job = build_job(add_args.clone(), Some(task_id))?;
            let response = client.add_job(job).await.context("Failed to add job")?;
            job_ids.push(response.id);
            println!(
                "Submitted batch job {} ({})",
                response.id, response.run_name
            );
        }
    } else {
        let job = build_job(add_args, None)?;
        let response = client.add_job(job).await.context("Failed to add job")?;
        println!(
            "Submitted batch job {} ({})",
            response.id, response.run_name
        );
    }

    Ok(())
}

/// Detects the currently active conda environment from the environment variables
fn detect_current_conda_env() -> Option<String> {
    env::var("CONDA_DEFAULT_ENV")
        .ok()
        .filter(|env_name| !env_name.is_empty())
}

fn build_job(args: cli::AddArgs, task_id: Option<u32>) -> Result<Job> {
    let mut builder = Job::builder();
    let run_dir = std::env::current_dir().context("Failed to get current directory")?;
    builder = builder.run_dir(run_dir);
    builder = builder.task_id(task_id);

    // Parse time limit if provided
    let time_limit = if let Some(time_str) = &args.time {
        Some(parse_time_limit(time_str)?)
    } else {
        None
    };

    if let Some(script) = &args.script {
        let script_path = make_absolute_path(script.clone())?;
        let script_args = parse_script_for_args(&script_path)?;

        builder = builder.script(script_path);
        builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
        builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
        builder = builder.conda_env(&args.conda_env.or(script_args.conda_env));
        builder = builder.depends_on(args.depends_on.or(script_args.depends_on));

        // CLI time limit takes precedence over script time limit
        let final_time_limit = if time_limit.is_some() {
            time_limit
        } else if let Some(script_time_str) = &script_args.time {
            Some(parse_time_limit(script_time_str)?)
        } else {
            None
        };
        builder = builder.time_limit(final_time_limit);
    } else if let Some(command) = args.command {
        builder = builder.command(command);
        builder = builder.gpus(args.gpus.unwrap_or(0));
        builder = builder.priority(args.priority.unwrap_or(10));

        // Auto-detect conda environment if not specified
        let conda_env = args.conda_env.or_else(detect_current_conda_env);
        builder = builder.conda_env(&conda_env);

        builder = builder.depends_on(args.depends_on);
        builder = builder.time_limit(time_limit);
    }

    Ok(builder.build())
}

/// Parse time limit string into Duration
/// Supported formats:
/// - "HH:MM:SS" - hours:minutes:seconds
/// - "MM:SS" - minutes:seconds
/// - "MM" - minutes
/// - raw number - seconds
fn parse_time_limit(time_str: &str) -> Result<Duration> {
    let parts: Vec<&str> = time_str.split(':').collect();

    match parts.len() {
        1 => {
            // Either minutes or seconds as a single number
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

fn parse_array_spec(spec: &str) -> Result<Vec<u32>> {
    if let Some(parts) = spec.split_once('-') {
        let start = parts
            .0
            .parse::<u32>()
            .context("Invalid array start index")?;
        let end = parts.1.parse::<u32>().context("Invalid array end index")?;
        if start > end {
            return Err(anyhow!(
                "Array start index cannot be greater than end index"
            ));
        }
        Ok((start..=end).collect())
    } else {
        Err(anyhow!(
            "Invalid array format. Expected format like '1-10'."
        ))
    }
}

fn parse_script_for_args(script_path: &PathBuf) -> Result<cli::AddArgs> {
    let content = fs::read_to_string(script_path).context("Failed to read script file")?;
    let gflow_lines: Vec<&str> = content
        .lines()
        .filter(|line| line.starts_with("# GFLOW"))
        .map(|line| line.trim_start_matches("# GFLOW").trim())
        .collect();

    if gflow_lines.is_empty() {
        return Ok(cli::AddArgs {
            script: None,
            command: None,
            conda_env: None,
            gpus: None,
            priority: None,
            depends_on: None,
            array: None,
            time: None,
        });
    }

    let args_str = gflow_lines.join(" ");
    let full_args = format!("gbatch {args_str}");
    let parsed = cli::GBatch::try_parse_from(full_args.split_whitespace())?;
    Ok(parsed.add_args)
}

fn make_absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|pwd| pwd.join(path))
            .context("Failed to get current directory")
    }
}
