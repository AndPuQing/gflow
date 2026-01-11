use anyhow::Result;
use gflow::client::Client;
use gflow::core::job::Job;
use gflow::utils::parse_job_ids;
use std::path::PathBuf;
use std::time::SystemTime;

pub async fn handle_show(config_path: &Option<PathBuf>, job_ids_str: String) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    let job_ids = parse_job_ids(&job_ids_str)?;

    for (index, &job_id) in job_ids.iter().enumerate() {
        if index > 0 {
            println!("\n{}", "=".repeat(80));
            println!();
        }

        let job = match client.get_job(job_id).await? {
            Some(job) => job,
            None => {
                eprintln!("Job {} not found.", job_id);
                continue;
            }
        };

        print_job_details(&job);
    }
    Ok(())
}

fn print_job_details(job: &Job) {
    println!("Job Details:");
    println!("  ID:            {}", job.id);
    println!(
        "  State:         {} ({})",
        job.state,
        job.state.short_form()
    );
    println!("  Priority:      {}", job.priority);
    println!("  Submitted by:  {}", job.submitted_by);

    // Command or script
    if let Some(ref script) = job.script {
        println!("  Script:        {}", script.display());
    }
    if let Some(ref command) = job.command {
        println!("  Command:       {}", command);
    }

    // Resources
    println!("\nResources:");
    println!("  GPUs:          {}", job.gpus);
    if let Some(ref gpu_ids) = job.gpu_ids {
        println!("  GPU IDs:       {}", format_gpu_ids(gpu_ids));
    }
    if let Some(memory_mb) = job.memory_limit_mb {
        println!(
            "  Memory limit:  {}",
            gflow::utils::format_memory(memory_mb)
        );
    }
    if let Some(ref conda_env) = job.conda_env {
        println!("  Conda env:     {}", conda_env);
    }

    // Working directory and run name
    println!("\nExecution:");
    println!("  Working dir:   {}", job.run_dir.display());
    if let Some(ref run_name) = job.run_name {
        println!("  Tmux session:  {}", run_name);
    }

    // Dependencies
    if let Some(depends_on) = job.depends_on {
        println!("\nDependencies:");
        println!("  Depends on:    {}", depends_on);
    }
    if let Some(task_id) = job.task_id {
        println!("  Task ID:       {}", task_id);
    }

    // Time information
    println!("\nTiming:");
    if let Some(time_limit) = job.time_limit {
        println!("  TimeLimit={}", gflow::utils::format_duration(time_limit));
    }
    if let Some(started_at) = job.started_at {
        println!("  StartTime={}", format_slurm_time(started_at));
        if let Some(finished_at) = job.finished_at {
            println!("  EndTime={}", format_slurm_time(finished_at));
            if let Ok(duration) = finished_at.duration_since(started_at) {
                println!("  Runtime={}", gflow::utils::format_duration(duration));
            }
        } else if job.state.to_string() == "Running" {
            if let Ok(elapsed) = SystemTime::now().duration_since(started_at) {
                println!("  Elapsed={}", gflow::utils::format_duration(elapsed));
            }
        }
    }
}

fn format_gpu_ids(gpu_ids: &[u32]) -> String {
    gpu_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn format_slurm_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local};

    let datetime: DateTime<Local> = time.into();
    datetime.format("%m/%d-%H:%M:%S").to_string()
}
