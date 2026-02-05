use anyhow::Result;
use gflow::core::job::Job;
use gflow::utils::{parse_job_ids, substitute_parameters};
use gflow::{print_field, print_optional_field};
use std::path::PathBuf;
use std::time::SystemTime;

pub async fn handle_show(config_path: &Option<PathBuf>, job_ids_str: String) -> Result<()> {
    let client = gflow::create_client(config_path)?;

    let job_ids = parse_job_ids(&job_ids_str)?;

    for (index, &job_id) in job_ids.iter().enumerate() {
        if index > 0 {
            println!("\n{}", "=".repeat(80));
            println!();
        }

        let Some(job) = gflow::client::get_job_or_warn(&client, job_id).await? else {
            continue;
        };

        print_job_details(&job);
    }
    Ok(())
}

fn print_job_details(job: &Job) {
    println!("Job Details:");
    print_field!("ID", "{}", job.id);
    print_field!("State", "{} ({})", job.state, job.state.short_form());
    print_field!("Priority", "{}", job.priority);
    print_field!("SubmittedBy", "{}", job.submitted_by);
    print_optional_field!("GroupID", job.group_id);

    // Command or script
    print_optional_field!("Script", job.script, |s| s.display());
    if let Some(ref command) = job.command {
        // Check if command contains parameters
        let has_params = command.contains('{') && !job.parameters.is_empty();

        if has_params {
            print_field!("Command(template)", "{}", command);
            match substitute_parameters(command, &job.parameters) {
                Ok(substituted) => print_field!("Command(actual)", "{}", substituted),
                Err(e) => print_field!("Command(actual)", "Error: {}", e),
            }
        } else {
            print_field!("Command", "{}", command);
        }
    }

    // Parameters
    if !job.parameters.is_empty() {
        println!("\nParameters:");
        let mut params: Vec<_> = job.parameters.iter().collect();
        params.sort_by_key(|(k, _)| *k);
        for (key, value) in params {
            print_field!(key, "{}", value);
        }
    }

    // Resources
    println!("\nResources:");
    print_field!("GPUs", "{}", job.gpus);
    print_optional_field!("GPUIDs", job.gpu_ids, |ids| format_ids(ids));
    if let Some(memory_mb) = job.memory_limit_mb {
        print_field!("MemoryLimit", "{}", gflow::utils::format_memory(memory_mb));
    }
    print_optional_field!("CondaEnv", job.conda_env);

    // Working directory and run name
    println!("\nExecution:");
    print_field!("WorkingDir", "{}", job.run_dir.display());
    print_optional_field!("TmuxSession", job.run_name);

    // Dependencies
    let all_deps = job.all_dependency_ids();
    if !all_deps.is_empty() || job.task_id.is_some() {
        println!("\nDependencies:");
        if !all_deps.is_empty() {
            print_field!("DependsOn", "{}", format_ids(&all_deps));
            if let Some(mode) = job.dependency_mode {
                print_field!("Mode", "{:?}", mode);
            }
            if job.auto_cancel_on_dependency_failure {
                print_field!("AutoCancel", "enabled");
            }
        }
        if let Some(task_id) = job.task_id {
            print_field!("TaskID", "{}", task_id);
        }
    }

    // Time information
    println!("\nTiming:");
    if let Some(time_limit) = job.time_limit {
        print_field!("TimeLimit", "{}", gflow::utils::format_duration(time_limit));
    }
    if let Some(started_at) = job.started_at {
        print_field!("StartTime", "{}", format_time(started_at));
        if let Some(finished_at) = job.finished_at {
            print_field!("EndTime", "{}", format_time(finished_at));
            if let Ok(duration) = finished_at.duration_since(started_at) {
                print_field!("Runtime", "{}", gflow::utils::format_duration(duration));
            }
        } else if job.state.to_string() == "Running" {
            if let Ok(elapsed) = SystemTime::now().duration_since(started_at) {
                print_field!("Elapsed", "{}", gflow::utils::format_duration(elapsed));
            }
        }
    }
}

/// Format a slice of u32 IDs as a comma-separated string
fn format_ids(ids: &[u32]) -> String {
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn format_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local};

    let datetime: DateTime<Local> = time.into();
    datetime.format("%m/%d-%H:%M:%S").to_string()
}
