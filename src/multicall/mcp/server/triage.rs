use gflow::core::job::{Job, JobState};
use gflow::Client;
use std::fs;
use std::time::SystemTime;

use super::helpers::serialize_job_value;
use super::log::{clean_terminal_output, extract_likely_program_output, slice_text, TextSlice};
use super::schemas::{TriageJobOutput, TriageJobRequest};

pub(super) async fn read_job_log_excerpt(
    client: &Client,
    job: &Job,
    params: &TriageJobRequest,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    let Some(log_path) = client.get_job_log_path(params.job_id).await? else {
        return Ok((None, None));
    };

    let raw = match fs::read_to_string(&log_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Some(log_path), None));
        }
        Err(err) => {
            anyhow::bail!("Failed to read log file '{}': {}", log_path, err);
        }
    };

    let slice = TextSlice::Last(params.last_lines.unwrap_or(80));
    let cleaned = slice_text(
        clean_terminal_output(&raw),
        slice,
        Some(params.max_bytes.unwrap_or(20_000)),
    );
    let program_output = extract_likely_program_output(&cleaned, job);
    let excerpt = if program_output.is_empty() {
        cleaned
    } else {
        program_output
    };

    Ok((Some(log_path), Some(excerpt)))
}

pub(super) fn build_triage_job_output(
    job: Job,
    log_path: Option<String>,
    log_excerpt: Option<String>,
) -> anyhow::Result<TriageJobOutput> {
    let retry_hints = retry_hints_for_job(&job, log_excerpt.as_deref());
    let output = TriageJobOutput {
        job_id: job.id,
        state: job.state.to_string(),
        reason: job.reason.as_deref().map(ToString::to_string),
        requested_gpus: job.gpus,
        gpu_ids: job.gpu_ids.as_ref().map(|ids| ids.to_vec()),
        runtime_secs: job_runtime_secs(&job),
        wait_secs: job_wait_secs(&job),
        exit_status: None,
        exit_status_note: "gflow currently records terminal state but not the process exit code"
            .to_string(),
        log_path,
        log_excerpt,
        retry_hints,
        job: serialize_job_value(&job),
    };
    Ok(output)
}

fn job_runtime_secs(job: &Job) -> Option<f64> {
    let start = job.started_at?;
    let end = job.finished_at.unwrap_or_else(SystemTime::now);
    duration_between_secs(start, end)
}

fn job_wait_secs(job: &Job) -> Option<f64> {
    let submitted = job.submitted_at?;
    let end = job.started_at.unwrap_or_else(SystemTime::now);
    duration_between_secs(submitted, end)
}

fn duration_between_secs(start: SystemTime, end: SystemTime) -> Option<f64> {
    end.duration_since(start)
        .ok()
        .map(|duration| duration.as_secs_f64())
}

fn retry_hints_for_job(job: &Job, log_excerpt: Option<&str>) -> Vec<String> {
    let mut hints = Vec::new();

    match job.state {
        JobState::Queued => match job.reason.as_deref().map(ToString::to_string) {
            Some(reason) if reason.contains("Dependency") => hints.push(
                "inspect dependency jobs before retrying or changing dependencies".to_string(),
            ),
            Some(reason) if reason.contains("Memory") => {
                hints.push("lower memory request or wait for memory pressure to clear".to_string())
            }
            Some(reason) if reason.contains("Gpu") || reason.contains("Resources") => hints.push(
                "check get_queue_pressure for GPU availability, reservations, and running jobs"
                    .to_string(),
            ),
            _ => hints.push("check get_queue_pressure before changing the job".to_string()),
        },
        JobState::Failed | JobState::Timeout => {
            hints.push("review the log excerpt before using redo_job".to_string());
            if job.max_retries > 0 {
                hints.push(format!(
                    "job has max_retries={} configured; check whether automatic retries already ran",
                    job.max_retries
                ));
            }
        }
        JobState::Cancelled => {
            hints.push("confirm why the job was cancelled before resubmitting".to_string());
        }
        JobState::Hold => {
            hints.push(
                "release_job can make this job schedulable after user confirmation".to_string(),
            );
        }
        JobState::Running => {
            hints.push("job is still running; inspect logs instead of retrying".to_string());
        }
        JobState::Finished => {
            hints.push("job finished successfully; retry is usually unnecessary".to_string());
        }
    }

    if let Some(log) = log_excerpt {
        let lower = log.to_ascii_lowercase();
        if lower.contains("out of memory") || lower.contains("cuda oom") {
            hints.push(
                "log suggests OOM; consider requesting more memory or reducing workload"
                    .to_string(),
            );
        }
        if lower.contains("no space left") {
            hints.push("log suggests disk pressure; free space before retrying".to_string());
        }
        if lower.contains("command not found") {
            hints.push("log suggests environment or PATH setup failure".to_string());
        }
    }

    hints.sort();
    hints.dedup();
    hints
}
