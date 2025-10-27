mod cli;
use anyhow::Result;
use clap::Parser;
use gflow::{client::Client, config::load_config, core::job::JobState};
use std::time::SystemTime;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GQueue::parse();
    let config = load_config(args.config.as_ref())?;

    let client = Client::build(&config)?;
    let mut jobs = client.list_jobs().await?;

    // Apply filters
    if let Some(states) = args.states {
        let states: Vec<JobState> = states
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !states.is_empty() {
            jobs.retain(|job| states.contains(&job.state));
        }
    }

    if let Some(job_ids) = args.jobs {
        let job_ids: Vec<u32> = job_ids
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !job_ids.is_empty() {
            jobs.retain(|job| job_ids.contains(&job.id));
        }
    }

    if let Some(names) = args.names {
        let names: Vec<String> = names.split(',').map(|s| s.trim().to_string()).collect();
        if !names.is_empty() {
            jobs.retain(|job| {
                job.run_name
                    .as_ref()
                    .is_some_and(|run_name| names.contains(run_name))
            });
        }
    }

    if jobs.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    // Sort jobs
    sort_jobs(&mut jobs, &args.sort);

    // Apply limit
    let limit = if args.all { 0 } else { args.limit };
    if limit > 0 {
        let limit_usize = limit as usize;
        if jobs.len() > limit_usize {
            let total_jobs = jobs.len();
            jobs.truncate(limit_usize);
            println!(
                "Showing {} of {} jobs (use --all or --limit 0 to show all)",
                limit, total_jobs
            );
            println!();
        }
    }

    // Group by state if requested
    if args.group {
        display_grouped_jobs(jobs, args.format.as_deref());
    } else {
        display_jobs_table(jobs, args.format.as_deref());
    }

    Ok(())
}

fn sort_jobs(jobs: &mut [gflow::core::job::Job], sort_field: &str) {
    match sort_field.to_lowercase().as_str() {
        "id" => jobs.sort_by_key(|j| j.id),
        "state" => jobs.sort_by_key(|j| j.state.clone()),
        "time" => jobs.sort_by(|a, b| a.started_at.cmp(&b.started_at)),
        "name" => jobs.sort_by(|a, b| {
            a.run_name
                .as_deref()
                .unwrap_or("")
                .cmp(b.run_name.as_deref().unwrap_or(""))
        }),
        "gpus" | "nodes" => jobs.sort_by_key(|j| j.gpus),
        "priority" => jobs.sort_by_key(|j| j.priority),
        _ => eprintln!(
            "Warning: Unknown sort field '{}', using default 'id'",
            sort_field
        ),
    }
}

fn display_jobs_table(jobs: Vec<gflow::core::job::Job>, format: Option<&str>) {
    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let headers: Vec<&str> = format.split(',').collect();

    println!(
        "{}",
        headers
            .iter()
            .map(|h| format!("{:<width$}", h, width = get_width(h)))
            .collect::<Vec<_>>()
            .join(" ")
    );

    for job in jobs {
        let mut row = Vec::new();
        for header in &headers {
            let value = match *header {
                "JOBID" => job.id.to_string(),
                "NAME" => job.run_name.as_deref().unwrap_or("-").to_string(),
                "ST" => job.state.short_form().to_string(),
                "NODES" => job.gpus.to_string(),
                "NODELIST(REASON)" => job.gpu_ids.as_ref().map_or_else(
                    || "-".to_string(),
                    |ids| {
                        ids.iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    },
                ),
                "TIME" => format_elapsed_time(job.started_at, job.finished_at),
                _ => "".to_string(),
            };
            row.push(format!("{:<width$}", value, width = get_width(header)));
        }
        println!("{}", row.join(" "));
    }
}

fn display_grouped_jobs(jobs: Vec<gflow::core::job::Job>, format: Option<&str>) {
    use gflow::core::job::JobState;

    let mut grouped = std::collections::HashMap::new();
    for job in jobs {
        grouped
            .entry(job.state.clone())
            .or_insert_with(Vec::new)
            .push(job);
    }

    let states_order = [
        JobState::Running,
        JobState::Queued,
        JobState::Finished,
        JobState::Failed,
        JobState::Cancelled,
    ];

    for state in states_order {
        if let Some(state_jobs) = grouped.get(&state) {
            println!("\n{} ({})", state, state_jobs.len());
            println!("{}", "─".repeat(60));
            display_jobs_table(state_jobs.clone(), format);
        }
    }
}

fn format_elapsed_time(started_at: Option<SystemTime>, finished_at: Option<SystemTime>) -> String {
    match started_at {
        Some(start_time) => {
            // For finished/failed jobs, use finished_at; for running jobs, use current time
            let end_time = finished_at.unwrap_or_else(SystemTime::now);

            if let Ok(elapsed) = end_time.duration_since(start_time) {
                let total_seconds = elapsed.as_secs();
                let days = total_seconds / 86400;
                let hours = (total_seconds % 86400) / 3600;
                let minutes = (total_seconds % 3600) / 60;
                let seconds = total_seconds % 60;

                if days > 0 {
                    format!("{}-{:02}:{:02}:{:02}", days, hours, minutes, seconds)
                } else {
                    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                }
            } else {
                "-".to_string()
            }
        }
        None => "-".to_string(),
    }
}

fn get_width(header: &str) -> usize {
    match header {
        "JOBID" => 8,
        "NAME" => 20,
        "ST" => 5,
        "TIME" => 12,
        "NODES" => 8,
        "NODELIST(REASON)" => 15,
        _ => 10,
    }
}
