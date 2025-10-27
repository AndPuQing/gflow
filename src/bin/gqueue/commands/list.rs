use anyhow::Result;
use gflow::{client::Client, core::job::JobState};
use std::time::SystemTime;

pub struct ListOptions {
    pub states: Option<String>,
    pub jobs: Option<String>,
    pub names: Option<String>,
    pub sort: String,
    pub limit: u32,
    pub all: bool,
    pub group: bool,
    pub format: Option<String>,
}

pub async fn handle_list(client: &Client, options: ListOptions) -> Result<()> {
    let mut jobs_vec = client.list_jobs().await?;

    // Apply filters
    if let Some(states_filter) = options.states {
        let states_vec: Vec<JobState> = states_filter
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !states_vec.is_empty() {
            jobs_vec.retain(|job| states_vec.contains(&job.state));
        }
    }

    if let Some(job_ids) = options.jobs {
        let job_ids_vec: Vec<u32> = job_ids
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !job_ids_vec.is_empty() {
            jobs_vec.retain(|job| job_ids_vec.contains(&job.id));
        }
    }

    if let Some(names_filter) = options.names {
        let names_vec: Vec<String> = names_filter
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        if !names_vec.is_empty() {
            jobs_vec.retain(|job| {
                job.run_name
                    .as_ref()
                    .is_some_and(|run_name| names_vec.contains(run_name))
            });
        }
    }

    if jobs_vec.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    // Sort jobs
    sort_jobs(&mut jobs_vec, &options.sort);

    // Apply limit
    let effective_limit = if options.all { 0 } else { options.limit };
    if effective_limit > 0 {
        let limit_usize = effective_limit as usize;
        if jobs_vec.len() > limit_usize {
            let total_jobs = jobs_vec.len();
            jobs_vec.truncate(limit_usize);
            println!(
                "Showing {} of {} jobs (use --all or --limit 0 to show all)",
                effective_limit, total_jobs
            );
            println!();
        }
    }

    // Group by state if requested
    if options.group {
        display_grouped_jobs(jobs_vec, options.format.as_deref());
    } else {
        display_jobs_table(jobs_vec, options.format.as_deref());
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
