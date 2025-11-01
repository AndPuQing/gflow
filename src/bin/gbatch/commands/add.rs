use crate::cli;
use crate::history::SubmissionHistory;
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
    let mut history =
        SubmissionHistory::load().context("Failed to load gbatch submission history")?;

    if let Some(array_spec) = &add_args.array {
        let task_ids = parse_array_spec(array_spec)?;
        let mut job_ids = Vec::new();
        for task_id in task_ids {
            let job = build_job(add_args.clone(), Some(task_id), &history)?;
            let response = client.add_job(job).await.context("Failed to add job")?;
            history
                .record(response.id)
                .context("Failed to persist submission history")?;
            job_ids.push(response.id);
            println!(
                "Submitted batch job {} ({})",
                response.id, response.run_name
            );
        }
    } else {
        let job = build_job(add_args, None, &history)?;
        let response = client.add_job(job).await.context("Failed to add job")?;
        println!(
            "Submitted batch job {} ({})",
            response.id, response.run_name
        );
        history
            .record(response.id)
            .context("Failed to persist submission history")?;
    }

    Ok(())
}

/// Detects the currently active conda environment from the environment variables
fn detect_current_conda_env() -> Option<String> {
    env::var("CONDA_DEFAULT_ENV")
        .ok()
        .filter(|env_name| !env_name.is_empty())
}

fn build_job(args: cli::AddArgs, task_id: Option<u32>, history: &SubmissionHistory) -> Result<Job> {
    let mut builder = Job::builder();
    let run_dir = std::env::current_dir().context("Failed to get current directory")?;
    builder = builder.run_dir(run_dir);
    builder = builder.task_id(task_id);

    // Get the username of the submitter
    let username = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    builder = builder.submitted_by(username);

    // Parse time limit if provided
    let time_limit = if let Some(time_str) = &args.time {
        Some(parse_time_limit(time_str)?)
    } else {
        None
    };

    // Determine if it's a script or command
    let is_script =
        args.script_or_command.len() == 1 && PathBuf::from(&args.script_or_command[0]).exists();

    if is_script {
        // Script mode
        let script_path = make_absolute_path(PathBuf::from(&args.script_or_command[0]))?;
        let script_args = parse_script_for_args(&script_path)?;

        builder = builder.script(script_path);
        builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
        builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
        builder = builder.conda_env(&args.conda_env.or(script_args.conda_env));

        let depends_on_expr = args.depends_on.or(script_args.depends_on);
        let depends_on = resolve_dependency(depends_on_expr, history)?;
        builder = builder.depends_on(depends_on);

        // CLI time limit takes precedence over script time limit
        let final_time_limit = if time_limit.is_some() {
            time_limit
        } else if let Some(script_time_str) = &script_args.time {
            Some(parse_time_limit(script_time_str)?)
        } else {
            None
        };
        builder = builder.time_limit(final_time_limit);
    } else {
        // Command mode
        let command = args.script_or_command.join(" ");
        builder = builder.command(command);
        builder = builder.gpus(args.gpus.unwrap_or(0));
        builder = builder.priority(args.priority.unwrap_or(10));

        // Auto-detect conda environment if not specified
        let conda_env = args.conda_env.or_else(detect_current_conda_env);
        builder = builder.conda_env(&conda_env);

        let depends_on = resolve_dependency(args.depends_on, history)?;
        builder = builder.depends_on(depends_on);
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
            script_or_command: vec![],
            conda_env: None,
            gpus: None,
            priority: None,
            depends_on: None,
            array: None,
            time: None,
        });
    }

    let args_str = gflow_lines.join(" ");
    // Add a dummy positional arg since we only care about the options
    let full_args = format!("gbatch {args_str} dummy");
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

fn resolve_dependency(
    depends_on: Option<String>,
    history: &SubmissionHistory,
) -> Result<Option<u32>> {
    match depends_on {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(anyhow!("Dependency value cannot be empty"));
            }
            if trimmed == "@" {
                return history
                    .recent(1)
                    .ok_or_else(|| anyhow!("No previous submissions found for '@' dependency"))
                    .map(Some);
            }

            if let Some(offset_str) = trimmed.strip_prefix("@~") {
                if offset_str.is_empty() {
                    return Err(anyhow!(
                        "Invalid dependency shorthand '@~' without an offset value"
                    ));
                }
                let offset = offset_str
                    .parse::<usize>()
                    .map_err(|_| anyhow!("Invalid offset value in dependency: {trimmed}"))?;
                if offset == 0 {
                    return Err(anyhow!("Dependency offset must be at least 1 (got 0)"));
                }
                return history
                    .recent(offset)
                    .ok_or_else(|| {
                        anyhow!(
                            "Only {} previous submission(s) recorded; cannot resolve '{}'",
                            history.len(),
                            trimmed
                        )
                    })
                    .map(Some);
            }

            let parsed = trimmed
                .parse::<u32>()
                .map_err(|_| anyhow!("Invalid dependency value: {trimmed}"))?;
            Ok(Some(parsed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use tempfile::tempdir;

    fn history_with(ids: &[u32]) -> (SubmissionHistory, tempfile::TempDir) {
        let temp_dir = tempdir().expect("tempdir");
        let mut history =
            SubmissionHistory::load_from_dir(temp_dir.path().to_path_buf()).expect("history");
        for &id in ids {
            history.record(id).expect("record");
        }
        (history, temp_dir)
    }

    #[test]
    fn resolves_numeric_dependency() -> Result<()> {
        let (history, _guard) = history_with(&[]);
        let resolved = resolve_dependency(Some("42".to_string()), &history)?;
        assert_eq!(resolved, Some(42));
        Ok(())
    }

    #[test]
    fn resolves_at_dependency_using_last_submission() -> Result<()> {
        let (history, _guard) = history_with(&[10, 11, 12]);
        let resolved = resolve_dependency(Some("@".to_string()), &history)?;
        assert_eq!(resolved, Some(12));
        Ok(())
    }

    #[test]
    fn resolves_at_offset_dependency() -> Result<()> {
        let (history, _guard) = history_with(&[101, 102, 103, 104]);
        let resolved = resolve_dependency(Some("@~3".to_string()), &history)?;
        assert_eq!(resolved, Some(102));
        Ok(())
    }

    #[test]
    fn errors_when_history_is_too_short() {
        let (history, _guard) = history_with(&[5]);
        let err = resolve_dependency(Some("@~2".to_string()), &history).unwrap_err();
        assert!(err
            .to_string()
            .contains("Only 1 previous submission(s) recorded"));
    }

    #[test]
    fn errors_on_zero_offset() {
        let (history, _guard) = history_with(&[20]);
        let err = resolve_dependency(Some("@~0".to_string()), &history).unwrap_err();
        assert!(err
            .to_string()
            .contains("Dependency offset must be at least 1"));
    }

    #[test]
    fn errors_on_invalid_shorthand() {
        let (history, _guard) = history_with(&[7]);
        let err = resolve_dependency(Some("@foo".to_string()), &history).unwrap_err();
        assert!(err.to_string().contains("Invalid dependency value: @foo"));
    }
}
