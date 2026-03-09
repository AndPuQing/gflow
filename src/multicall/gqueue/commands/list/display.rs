use gflow::core::job::{GpuIds, JobState};
use owo_colors::OwoColorize;
use std::collections::HashSet;
use tabled::{builder::Builder, settings::style::Style};

pub(super) fn display_jobs_table(
    jobs: &[gflow::core::job::Job],
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let headers: Vec<&str> = format.split(',').collect();

    // Build table using tabled Builder
    let mut builder = Builder::default();

    // Add header row
    builder.push_record(headers.clone());

    // Add data rows
    for job in jobs {
        let row: Vec<String> = headers
            .iter()
            .map(|header| format_job_cell(job, header, tmux_sessions))
            .collect();
        builder.push_record(row);
    }

    let mut table = builder.build();
    table.with(Style::blank());

    println!("{}", table);
}

/// Displays jobs in a standard table format (for references)
fn display_jobs_table_refs(
    jobs: &[&gflow::core::job::Job],
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let headers: Vec<&str> = format.split(',').collect();

    // Build table using tabled Builder
    let mut builder = Builder::default();

    // Add header row
    builder.push_record(headers.clone());

    // Add data rows
    for job in jobs {
        let row: Vec<String> = headers
            .iter()
            .map(|header| format_job_cell(job, header, tmux_sessions))
            .collect();
        builder.push_record(row);
    }

    let mut table = builder.build();
    table.with(Style::blank());

    println!("{}", table);
}

pub(super) fn display_grouped_jobs(
    jobs: &[gflow::core::job::Job],
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
) {
    use gflow::core::job::JobState;

    let mut grouped: std::collections::HashMap<JobState, Vec<&gflow::core::job::Job>> =
        std::collections::HashMap::new();
    for job in jobs {
        grouped.entry(job.state).or_default().push(job);
    }

    let states_order = [
        JobState::Running,
        JobState::Queued,
        JobState::Finished,
        JobState::Failed,
        JobState::Cancelled,
        JobState::Timeout,
    ];

    let mut first = true;
    for state in states_order {
        if let Some(state_jobs) = grouped.get(&state) {
            if !first {
                println!();
            }
            first = false;

            println!("{} ({})", state, state_jobs.len());
            println!("{}", "─".repeat(60));
            display_jobs_table_refs(state_jobs, format, tmux_sessions);
        }
    }
}

/// Colorizes a job state string based on its state
fn colorize_state(state: &JobState) -> String {
    let short = state.short_form();
    match state {
        JobState::Running => short.green().bold().to_string(),
        JobState::Finished => short.dimmed().to_string(),
        JobState::Queued => short.italic().to_string(),
        JobState::Hold => short.bold().to_string(),
        JobState::Failed => short.red().bold().to_string(),
        JobState::Timeout => short.underline().to_string(),
        JobState::Cancelled => short.strikethrough().to_string(),
    }
}

/// Computes the reason why a job is in its current state for display
pub(super) fn get_job_reason_display(job: &gflow::core::job::Job) -> String {
    use gflow::core::job::JobStateReason;

    // If job already has a reason set, use it (except for CancelledByUser)
    if let Some(reason) = job.reason.as_deref() {
        if matches!(reason, JobStateReason::CancelledByUser) {
            return "-".to_string();
        }
        return format!("({})", reason);
    }

    // Compute the reason based on state
    match job.state {
        JobState::Hold => format!("({})", JobStateReason::JobHeldUser),
        JobState::Queued => {
            let has_dependencies = job.depends_on.is_some() || !job.depends_on_ids.is_empty();
            if has_dependencies {
                format!("({})", JobStateReason::WaitingForDependency)
            } else {
                format!("({})", JobStateReason::WaitingForResources)
            }
        }
        JobState::Cancelled => "-".to_string(),
        _ => "-".to_string(),
    }
}

/// Formats GPU IDs as a comma-separated string
fn format_gpu_ids(gpu_ids: Option<&GpuIds>) -> String {
    gpu_ids.map_or_else(
        || "-".to_string(),
        |ids| {
            ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        },
    )
}

/// Formats a job field value for display
pub(super) fn format_job_cell(
    job: &gflow::core::job::Job,
    header: &str,
    tmux_sessions: &HashSet<String>,
) -> String {
    match header {
        "JOBID" => job.id.to_string(),
        "NAME" => format_job_name_with_session_status(job, tmux_sessions),
        "ST" => colorize_state(&job.state),
        "NODES" => job.gpus.to_string(),
        "MEMORY" => job
            .memory_limit_mb
            .map_or_else(|| "-".to_string(), gflow::utils::format_memory),
        "NODELIST(REASON)" => {
            // For running jobs, show GPU IDs
            // For queued/held/cancelled jobs, show pending reason
            match job.state {
                JobState::Running => format_gpu_ids(job.gpu_ids.as_ref()),
                JobState::Queued | JobState::Hold | JobState::Cancelled => {
                    get_job_reason_display(job)
                }
                _ => "-".to_string(),
            }
        }
        "TIME" => gflow::utils::format_elapsed_time(job.started_at, job.finished_at),
        "TIMELIMIT" => job
            .time_limit
            .map_or_else(|| "UNLIMITED".to_string(), gflow::utils::format_duration),
        "USER" => job.submitted_by.to_string(),
        "PROJECT" => job
            .project
            .as_ref()
            .map_or_else(|| "-".to_string(), |p| p.to_string()),
        _ => String::new(),
    }
}

/// Formats the job name with a visual indicator for tmux session status
fn format_job_name_with_session_status(
    job: &gflow::core::job::Job,
    tmux_sessions: &HashSet<String>,
) -> String {
    let Some(name) = &job.run_name else {
        return "-".to_string();
    };

    if tmux_sessions.contains(name.as_str()) {
        format!("{} {}", name, "○".green())
    } else {
        name.to_string()
    }
}
