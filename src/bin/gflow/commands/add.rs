use crate::{cli, client::Client};
use anyhow::{Context, Result};
use clap::Parser;
use gflow::core::job::Job;
use std::{fs, path::PathBuf};

pub(crate) async fn handle_add(config: &config::Config, add_args: cli::AddArgs) -> Result<()> {
    log::debug!("{:?}", add_args);

    let job = build_job(add_args)?;
    let client = Client::build(config).context("Failed to build client")?;

    client.add_job(job).await.context("Failed to add job")?;

    log::info!("Job added successfully");
    Ok(())
}

fn build_job(args: cli::AddArgs) -> Result<Job> {
    let mut builder = Job::builder();
    let run_dir = std::env::current_dir().context("Failed to get current directory")?;
    builder = builder.run_dir(run_dir);

    if let Some(script) = &args.script {
        let script_path = make_absolute_path(script.clone())?;
        let script_args = parse_script_for_args(&script_path)?;

        builder = builder.script(script_path);
        builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
        builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
        builder = builder.gpu_mem(args.gpu_mem.or(script_args.gpu_mem).unwrap_or(0));
        builder = builder.conda_env(&args.conda_env.or(script_args.conda_env));
        builder = builder.depends_on(args.depends_on.or(script_args.depends_on));
    } else if let Some(command) = args.command {
        builder = builder.command(command);
        builder = builder.gpus(args.gpus.unwrap_or(0));
        builder = builder.priority(args.priority.unwrap_or(10));
        builder = builder.gpu_mem(args.gpu_mem.unwrap_or(0));
        builder = builder.conda_env(&args.conda_env);
        builder = builder.depends_on(args.depends_on);
    }

    Ok(builder.build())
}

fn parse_script_for_args(script_path: &PathBuf) -> Result<cli::AddArgs> {
    let content = fs::read_to_string(script_path).context("Failed to read script file")?;
    let gflow_lines: Vec<&str> = content
        .lines()
        .filter(|line| line.starts_with("# GFLOW"))
        .map(|line| line.trim_start_matches("# GFLOW").trim())
        .collect();

    let args_str = gflow_lines.join(" ");
    let full_args = format!("gflow add {args_str}");
    let parsed = cli::GFlow::try_parse_from(full_args.split_whitespace())?;
    if let Some(cli::Commands::Add(add_args)) = parsed.commands {
        Ok(add_args)
    } else {
        Ok(cli::AddArgs {
            script: None,
            command: None,
            conda_env: None,
            gpus: None,
            priority: None,
            gpu_mem: None,
            depends_on: None,
        })
    }
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
}
