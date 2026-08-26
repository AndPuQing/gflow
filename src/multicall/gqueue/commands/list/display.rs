use gflow::core::job::{GpuIds, JobState};
use owo_colors::OwoColorize;
use std::collections::HashSet;
use std::io::IsTerminal;
use tabled::{
    builder::Builder,
    settings::{object::Columns, peaker::PriorityMax, style::Style, width::Width, Modify},
    Table,
};

/// How the job-name liveness indicator should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExecutorDisplay {
    /// Legacy tmux executor: show a green ○ when the job's tmux session is
    /// alive (queried client-side).
    TmuxSessions,
    /// Process executor: show a green ○ when the daemon reports the process
    /// alive; no indicator otherwise.
    ProcessLiveness,
}

/// A parsed `-f/--format` token: a field name plus an optional explicit
/// column width (e.g. `COMMAND`, `COMMAND:60`, `COMMAND:0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldSpec {
    pub(super) name: String,
    /// Explicitly requested max content width. `Some(0)` means unlimited;
    /// `None` means the user gave no width for this field.
    pub(super) width: Option<usize>,
}

impl FieldSpec {
    fn has_explicit_width(&self) -> bool {
        self.width.is_some()
    }
}

/// Parses a `-f/--format` value into field specs.
///
/// Each comma-separated token is a field name optionally followed by
/// `:WIDTH`, where `WIDTH` is a non-negative integer (`0` = unlimited,
/// `full` is accepted as an alias). An invalid width is ignored and the
/// token is treated as a plain field name, matching the lenient handling of
/// unknown fields.
pub(super) fn parse_field_specs(format: &str) -> Vec<FieldSpec> {
    format
        .split(',')
        .map(|token| match token.split_once(':') {
            Some((name, width)) => {
                let normalized = width.trim().to_ascii_lowercase();
                let width = if normalized == "full" {
                    Some(0)
                } else {
                    normalized.parse::<usize>().ok()
                };
                FieldSpec {
                    name: name.trim().to_string(),
                    width,
                }
            }
            None => FieldSpec {
                name: token.trim().to_string(),
                width: None,
            },
        })
        .collect()
}

/// Width (in columns) of the terminal stdout is attached to, when stdout is a
/// TTY. Returns `None` when stdout is redirected (pipe/file) or the size can't
/// be determined — in that case tables must not be truncated, so that piping
/// `gqueue` or redirecting it to a file keeps the full content.
pub(super) fn terminal_max_width() -> Option<usize> {
    if !std::io::stdout().is_terminal() {
        return None;
    }
    terminal_size::terminal_size().map(|(width, _)| width.0 as usize)
}

/// Applies styling and width handling to a built table.
///
/// - Explicit `FIELD:WIDTH` caps from the format string are enforced per
///   column with a `…` suffix; `FIELD:0` leaves that column untouched.
/// - When any explicit width is given, the automatic terminal-width fitting
///   is skipped — the caller takes manual control of the widths.
/// - Otherwise, with `Some(max)` from a TTY stdout, the table is truncated so
///   its total width never exceeds `max`. The widest columns are cut first
///   (`PriorityMax`), so a long `COMMAND` column gets trimmed while short
///   columns like `JOBID`/`ST` stay intact.
pub(super) fn finish_table_with(table: &mut Table, specs: &[FieldSpec], term_width: Option<usize>) {
    table.with(Style::blank());

    let has_explicit_width = specs.iter().any(FieldSpec::has_explicit_width);
    for (idx, spec) in specs.iter().enumerate() {
        match spec.width {
            None | Some(0) => {}
            Some(w) => {
                table.with(Modify::new(Columns::one(idx)).with(Width::truncate(w).suffix("…")));
            }
        }
    }

    if !has_explicit_width {
        if let Some(max) = term_width {
            table.with(
                Width::truncate(max)
                    .priority(PriorityMax::right())
                    .suffix("…"),
            );
        }
    }
}

pub(super) fn display_jobs_table(
    jobs: &[gflow::core::job::Job],
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
    executor: ExecutorDisplay,
) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let specs = parse_field_specs(&format);

    // Build table using tabled Builder
    let mut builder = Builder::default();

    // Add header row
    builder.push_record(specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>());

    // Add data rows
    for job in jobs {
        let row: Vec<String> = specs
            .iter()
            .map(|spec| format_job_cell(job, &spec.name, tmux_sessions, executor))
            .collect();
        builder.push_record(row);
    }

    let mut table = builder.build();
    finish_table_with(&mut table, &specs, terminal_max_width());

    println!("{}", table);
}

/// Displays jobs in a standard table format (for references)
fn display_jobs_table_refs(
    jobs: &[&gflow::core::job::Job],
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
    executor: ExecutorDisplay,
) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let specs = parse_field_specs(&format);

    // Build table using tabled Builder
    let mut builder = Builder::default();

    // Add header row
    builder.push_record(specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>());

    // Add data rows
    for job in jobs {
        let row: Vec<String> = specs
            .iter()
            .map(|spec| format_job_cell(job, &spec.name, tmux_sessions, executor))
            .collect();
        builder.push_record(row);
    }

    let mut table = builder.build();
    finish_table_with(&mut table, &specs, terminal_max_width());

    println!("{}", table);
}

pub(super) fn display_grouped_jobs(
    jobs: &[gflow::core::job::Job],
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
    executor: ExecutorDisplay,
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
            display_jobs_table_refs(state_jobs, format, tmux_sessions, executor);
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
        JobState::Queued => format!("({})", JobStateReason::WaitingForResources),
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
    executor: ExecutorDisplay,
) -> String {
    match header {
        "JOBID" => job.id.to_string(),
        "NAME" => format_job_name_with_session_status(job, tmux_sessions, executor),
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
        // What the job runs: script jobs execute `bash <script>`; command jobs
        // run the stored command. Script wins when both are present — matches
        // both executors (ProcessExecutor / TmuxExecutor).
        "COMMAND" => {
            if let Some(script) = &job.script {
                script.display().to_string()
            } else if let Some(command) = &job.command {
                command.to_string()
            } else {
                "-".to_string()
            }
        }
        _ => String::new(),
    }
}

/// Formats the job name with a visual liveness indicator.
///
/// - tmux executor: green ○ when the job's tmux session is alive.
/// - process executor: green ○ when the daemon reports the process alive;
///   nothing when the process is gone (the zombie monitor handles it).
fn format_job_name_with_session_status(
    job: &gflow::core::job::Job,
    tmux_sessions: &HashSet<String>,
    executor: ExecutorDisplay,
) -> String {
    let Some(name) = &job.run_name else {
        return "-".to_string();
    };

    match executor {
        ExecutorDisplay::TmuxSessions => {
            if tmux_sessions.contains(name.as_str()) {
                format!("{} {}", name, "○".green())
            } else {
                name.to_string()
            }
        }
        ExecutorDisplay::ProcessLiveness => {
            if job.alive == Some(true) {
                format!("{} {}", name, "○".green())
            } else {
                name.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::{Job, JobState};
    use std::path::PathBuf;

    fn running_job(name: &str) -> Job {
        Job {
            id: 1,
            run_name: Some(name.into()),
            state: JobState::Running,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        }
    }

    #[test]
    fn tmux_mode_shows_circle_only_for_live_sessions() {
        let mut sessions = HashSet::new();
        sessions.insert("gjob-1".to_string());

        let alive = running_job("gjob-1");
        let dead = running_job("gjob-2");

        let name =
            format_job_name_with_session_status(&alive, &sessions, ExecutorDisplay::TmuxSessions);
        assert!(
            name.contains("○"),
            "live session should show a circle: {name}"
        );

        let name =
            format_job_name_with_session_status(&dead, &sessions, ExecutorDisplay::TmuxSessions);
        assert_eq!(name, "gjob-2");
    }

    #[test]
    fn process_mode_uses_daemon_liveness_hint() {
        let sessions = HashSet::new();

        let mut alive = running_job("gjob-1");
        alive.alive = Some(true);
        let mut dead = running_job("gjob-2");
        dead.alive = Some(false);
        let unknown = running_job("gjob-3");

        let name = format_job_name_with_session_status(
            &alive,
            &sessions,
            ExecutorDisplay::ProcessLiveness,
        );
        assert!(name.contains("○"), "alive process should show ○: {name}");

        // Dead / unknown processes show no indicator at all.
        let name =
            format_job_name_with_session_status(&dead, &sessions, ExecutorDisplay::ProcessLiveness);
        assert_eq!(name, "gjob-2");

        let name = format_job_name_with_session_status(
            &unknown,
            &sessions,
            ExecutorDisplay::ProcessLiveness,
        );
        assert_eq!(name, "gjob-3", "no hint -> no indicator");
    }

    #[test]
    fn command_field_prefers_script_then_command_then_dash() {
        let sessions = HashSet::new();

        // Command job shows the stored command.
        let mut cmd_job = running_job("cmd");
        cmd_job.command = Some("python train.py --lr 0.001".into());
        assert_eq!(
            format_job_cell(
                &cmd_job,
                "COMMAND",
                &sessions,
                ExecutorDisplay::ProcessLiveness
            ),
            "python train.py --lr 0.001"
        );

        // Script job shows the script path.
        let mut script_job = running_job("script");
        script_job.script = Some(Box::new(PathBuf::from("/home/u/train.sh")));
        assert_eq!(
            format_job_cell(
                &script_job,
                "COMMAND",
                &sessions,
                ExecutorDisplay::ProcessLiveness
            ),
            "/home/u/train.sh"
        );

        // Both present: script wins (matches the executors).
        let mut both = running_job("both");
        both.script = Some(Box::new(PathBuf::from("/home/u/run.sh")));
        both.command = Some("python train.py".into());
        assert_eq!(
            format_job_cell(
                &both,
                "COMMAND",
                &sessions,
                ExecutorDisplay::ProcessLiveness
            ),
            "/home/u/run.sh"
        );

        // Neither: dash.
        let none = running_job("none");
        assert_eq!(
            format_job_cell(
                &none,
                "COMMAND",
                &sessions,
                ExecutorDisplay::ProcessLiveness
            ),
            "-"
        );
    }

    #[test]
    fn parse_field_specs_handles_width_suffixes() {
        let specs = parse_field_specs("JOBID,COMMAND:60,COMMAND:0,NAME:FULL,NODELIST(REASON)");
        assert_eq!(specs[0].name, "JOBID");
        assert_eq!(specs[0].width, None);
        assert_eq!(specs[1].name, "COMMAND");
        assert_eq!(specs[1].width, Some(60));
        assert_eq!(specs[2].width, Some(0));
        assert_eq!(specs[3].name, "NAME");
        assert_eq!(specs[3].width, Some(0), ":full is an alias for :0");
        assert_eq!(specs[4].name, "NODELIST(REASON)");
        assert_eq!(specs[4].width, None);

        // Invalid widths are ignored (lenient, like unknown fields).
        let bad = parse_field_specs("USER:abc");
        assert_eq!(
            bad[0],
            FieldSpec {
                name: "USER".into(),
                width: None,
            }
        );
    }

    #[test]
    fn table_fit_truncates_long_columns_to_max_width() {
        use tabled::builder::Builder;

        let specs = parse_field_specs("JOBID,COMMAND");
        let mut builder = Builder::default();
        builder.push_record(["JOBID", "COMMAND"]);
        builder.push_record([
            "1",
            "python train.py --config configs/exp1.yaml --lr 1e-4 --batch-size 128",
        ]);
        let mut table = builder.build();

        finish_table_with(&mut table, &specs, Some(30));

        let rendered = table.to_string();
        let data_row = rendered.lines().nth(1).unwrap();
        assert!(
            data_row.chars().count() <= 30,
            "table should fit the max width: {rendered}"
        );
        assert!(
            data_row.contains("python train.py --c…"),
            "long content should be truncated with an ellipsis: {rendered}"
        );
        assert!(
            !data_row.contains("batch-size 128"),
            "content beyond the limit must be cut: {rendered}"
        );
        // The short JOBID column must survive truncation.
        assert!(
            data_row.trim_start().starts_with("1 "),
            "JOBID should be intact: {rendered}"
        );
    }

    #[test]
    fn explicit_column_width_caps_that_column() {
        use tabled::builder::Builder;

        let specs = parse_field_specs("JOBID,COMMAND:20");
        let mut builder = Builder::default();
        builder.push_record(["JOBID", "COMMAND"]);
        builder.push_record([
            "1",
            "python train.py --config configs/exp1.yaml --lr 1e-4 --batch-size 128",
        ]);
        let mut table = builder.build();

        // Even with a narrow terminal available, an explicit width takes over.
        finish_table_with(&mut table, &specs, Some(10));

        let rendered = table.to_string();
        let data_row = rendered.lines().nth(1).unwrap();
        assert!(
            data_row.contains('…'),
            "the column should be capped with an ellipsis: {rendered}"
        );
        assert!(
            !data_row.contains("train.py --config"),
            "content beyond the cap must be cut: {rendered}"
        );
        assert!(
            data_row.trim_start().starts_with("1 "),
            "JOBID should be intact: {rendered}"
        );
    }

    #[test]
    fn explicit_unlimited_width_keeps_full_content() {
        use tabled::builder::Builder;

        let command = "python train.py --config configs/exp1.yaml --lr 1e-4 --batch-size 128";
        for token in ["JOBID,COMMAND:0", "JOBID,COMMAND:full"] {
            let specs = parse_field_specs(token);
            let mut builder = Builder::default();
            builder.push_record(["JOBID", "COMMAND"]);
            builder.push_record(["1", command]);
            let mut table = builder.build();

            // Even with a narrow terminal available, `:0`/`:full` wins.
            finish_table_with(&mut table, &specs, Some(10));

            let rendered = table.to_string();
            assert!(
                rendered.contains(command),
                "{token} must keep the full content: {rendered}"
            );
        }
    }

    #[test]
    fn table_fit_without_width_keeps_full_content() {
        use tabled::builder::Builder;

        let command = "python train.py --config configs/exp1.yaml --lr 1e-4 --batch-size 128";
        let specs = parse_field_specs("JOBID,COMMAND");
        let mut builder = Builder::default();
        builder.push_record(["JOBID", "COMMAND"]);
        builder.push_record(["1", command]);
        let mut table = builder.build();

        finish_table_with(&mut table, &specs, None);

        let rendered = table.to_string();
        assert!(
            rendered.contains(command),
            "without a width limit the full content must be kept: {rendered}"
        );
    }
}
