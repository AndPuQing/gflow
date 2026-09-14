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

        // The log summary is derived from the local log file, which lives on
        // the same host as the daemon that serves this client.
        let log_summary = client
            .get_job_log_path(job_id)
            .await
            .ok()
            .flatten()
            .and_then(|path| gflow::utils::logfile::summarize(std::path::Path::new(&path)));

        print_job_details(&job, log_summary.as_ref());
    }
    Ok(())
}

fn print_job_details(job: &Job, log_summary: Option<&gflow::utils::logfile::LogSummary>) {
    println!("Job Details:");
    print_field!("ID", "{}", job.id);
    print_field!("State", "{} ({})", job.state, job.state.short_form());
    print_field!("Priority", "{}", job.priority);
    print_field!("SubmittedBy", "{}", job.submitted_by);
    if job.max_retries > 0 {
        print_field!("MaxRetries", "{}", job.max_retries);
    }
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
    print_field!(
        "GPUSharing",
        "{}",
        format_gpu_sharing_mode(job.gpu_sharing_mode)
    );
    if let Some(gpu_memory_mb) = job.gpu_memory_limit_mb {
        print_field!(
            "GPUMemoryLimit",
            "{}",
            gflow::utils::format_memory(gpu_memory_mb)
        );
    }
    print_optional_field!("GPUIDs", job.gpu_ids, |ids| format_ids(ids));
    if let Some(memory_mb) = job.memory_limit_mb {
        print_field!("MemoryLimit", "{}", gflow::utils::format_memory(memory_mb));
    }
    print_optional_field!("CondaEnv", job.conda_env);

    // Working directory and run name
    println!("\nExecution:");
    print_field!("WorkingDir", "{}", job.run_dir.display());
    print_optional_field!("TmuxSession", job.run_name);
    if !job.notifications.is_empty() {
        print_field!(
            "NotifyEmail",
            "{}",
            job.notifications
                .emails
                .iter()
                .map(|email| email.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        if !job.notifications.events.is_empty() {
            print_field!(
                "NotifyOn",
                "{}",
                job.notifications
                    .events
                    .iter()
                    .map(|event| event.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }

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
    if let Some(submitted_at) = job.submitted_at {
        if let Some(wait_time) = job.wait_time() {
            print_field!(
                "Submitted",
                "{} (waited {})",
                format_time(submitted_at),
                gflow::utils::format_duration(wait_time)
            );
        } else {
            print_field!("Submitted", "{}", format_time(submitted_at));
        }
    }
    if let Some(started_at) = job.started_at {
        if let Some(finished_at) = job.finished_at {
            let runtime = finished_at.duration_since(started_at).ok();
            print_field!("Started", "{}", format_time(started_at));
            if let Some(rt) = runtime {
                print_field!(
                    "Finished",
                    "{} (runtime {})",
                    format_time(finished_at),
                    gflow::utils::format_duration(rt)
                );
            } else {
                print_field!("Finished", "{}", format_time(finished_at));
            }
        } else if let Ok(elapsed) = SystemTime::now().duration_since(started_at) {
            print_field!(
                "Started",
                "{} (running {} so far)",
                format_time(started_at),
                gflow::utils::format_duration(elapsed)
            );
        } else {
            print_field!("Started", "{}", format_time(started_at));
        }
    }

    // Progress published by the job itself (see `gjob progress`). Absent for
    // jobs that do not publish any; the log block below still shows liveness.
    if let Some(progress) = &job.progress {
        println!("\nProgress:");
        match progress.total {
            Some(total) => print_field!(
                "Value",
                "{}/{} ({})",
                progress.value,
                total,
                progress
                    .percent_display()
                    .unwrap_or_else(|| "?".to_string())
            ),
            None => print_field!("Value", "{}", progress.value),
        }
        if let Some(rate) = progress.rate_per_sec {
            print_field!("Rate", "{:.2}/s", rate);
        }
        match progress.eta_secs {
            Some(0) => print_field!("ETA", "done"),
            Some(eta) => {
                let remaining = gflow::utils::format_duration(std::time::Duration::from_secs(eta));
                // The estimate is work remaining at the observed rate, so it
                // only maps to a wall-clock time when the last measurement is
                // fresh. A stale estimate is still useful (it bounds the work
                // left) but must not be presented as a predicted finish time.
                if progress.stale {
                    print_field!("ETA", "{} (~, from stale progress)", remaining);
                } else {
                    print_field!(
                        "ETA",
                        "{} (at {})",
                        remaining,
                        format_time(SystemTime::now() + std::time::Duration::from_secs(eta))
                    );
                }
            }
            None => {}
        }
        if let Some(message) = &progress.message {
            print_field!("Message", "{}", message);
        }
        print_field!(
            "Updated",
            "{} ({} ago){}",
            format_time(progress.updated_at),
            gflow::utils::format_duration(std::time::Duration::from_secs(progress.idle_secs)),
            if progress.stale { " [STALE]" } else { "" }
        );
        if progress.stale {
            print_field!(
                "StaleHint",
                "no update for over {}",
                gflow::utils::format_duration(std::time::Duration::from_secs(
                    gflow::core::job::PROGRESS_STALE_AFTER_SECS
                ))
            );
        }
    }

    // Log liveness: mtime and the last line answer "is it still moving?"
    // without making the user parse the log.
    if let Some(summary) = log_summary {
        println!("\nLog:");
        print_field!("Path", "{}", summary.path.display());
        print_field!("Size", "{}", format_bytes(summary.size_bytes));
        if let Some(modified_at) = summary.modified_at {
            let idle = summary
                .idle_secs()
                .map(|secs| {
                    format!(
                        " (last write {} ago)",
                        gflow::utils::format_duration(std::time::Duration::from_secs(secs))
                    )
                })
                .unwrap_or_default();
            print_field!("Modified", "{}{}", format_time(modified_at), idle);
        }
        if let Some(last_line) = &summary.last_line {
            print_field!("LastLine", "{}", last_line);
        }
    }
}

/// Render a byte count with a binary unit suffix.
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.1}G", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1}M", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.1}K", bytes_f / KIB)
    } else {
        format!("{}B", bytes)
    }
}

/// Format a slice of u32 IDs as a comma-separated string
fn format_ids(ids: &[u32]) -> String {
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn format_gpu_sharing_mode(mode: gflow::core::job::GpuSharingMode) -> &'static str {
    match mode {
        gflow::core::job::GpuSharingMode::Exclusive => "exclusive",
        gflow::core::job::GpuSharingMode::Shared => "shared",
    }
}

fn format_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local};

    let datetime: DateTime<Local> = time.into();
    datetime.format("%m/%d-%H:%M:%S").to_string()
}
