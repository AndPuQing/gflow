use crate::cli;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use gflow::client::Client;
use gflow::core::job::Job;
use std::{fs, path::PathBuf};

pub(crate) async fn handle_add(config: &config::Config, add_args: cli::AddArgs) -> Result<()> {
    log::debug!("{:?}", add_args);

    let client = Client::build(config).context("Failed to build client")?;

    if let Some(array_spec) = &add_args.array {
        let task_ids = parse_array_spec(array_spec)?;
        println!("Submitting job array with {} tasks.", task_ids.len());
        for task_id in task_ids {
            let job = build_job(add_args.clone(), Some(task_id))?;
            client.add_job(job).await.context("Failed to add job")?;
        }
        println!("Successfully submitted job array.");
    } else {
        let job = build_job(add_args, None)?;
        client.add_job(job).await.context("Failed to add job")?;
        println!("Job added successfully.");
    }

    Ok(())
}

fn build_job(args: cli::AddArgs, task_id: Option<u32>) -> Result<Job> {
    let mut builder = Job::builder();
    let run_dir = std::env::current_dir().context("Failed to get current directory")?;
    builder = builder.run_dir(run_dir);
    builder = builder.task_id(task_id);

    if let Some(script) = &args.script {
        let script_path = make_absolute_path(script.clone())?;
        let script_args = parse_script_for_args(&script_path)?;

        builder = builder.script(script_path);
        builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
        builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
        builder = builder.conda_env(&args.conda_env.or(script_args.conda_env));
        builder = builder.depends_on(args.depends_on.or(script_args.depends_on));
    } else if let Some(command) = args.command {
        builder = builder.command(command);
        builder = builder.gpus(args.gpus.unwrap_or(0));
        builder = builder.priority(args.priority.unwrap_or(10));
        builder = builder.conda_env(&args.conda_env);
        builder = builder.depends_on(args.depends_on);
    }

    Ok(builder.build())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_make_absolute_path() {
        let path = Path::new("test.txt").to_path_buf();
        let result = make_absolute_path(path.clone()).unwrap();
        assert_eq!(result, std::env::current_dir().unwrap().join(path));
    }

    #[test]
    fn test_parse_array_spec() {
        assert_eq!(parse_array_spec("1-5").unwrap(), vec![1, 2, 3, 4, 5]);
        assert!(parse_array_spec("5-1").is_err());
        assert!(parse_array_spec("abc").is_err());
        assert!(parse_array_spec("1-abc").is_err());
    }
}
