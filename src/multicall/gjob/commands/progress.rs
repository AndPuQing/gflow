use anyhow::Result;
use gflow::core::job::progress::JobProgress;
use std::path::PathBuf;

/// Publish progress for a running job through the daemon API.
///
/// The daemon owns the progress directory (it lives under the data dir of the
/// account running `gflowd`), so going through the API is what makes progress
/// publishing work for every user on a shared host. It also means a job can
/// call this without knowing anything about the daemon's filesystem layout.
///
/// The job ID is taken from the argument, or from `GFLOW_JOB_ID` when the job
/// was started by gflow, so the common in-job call is just
/// `gjob progress -v 100 -t 1000`.
pub async fn handle_progress(
    config_path: &Option<PathBuf>,
    job_id_str: Option<String>,
    value: u64,
    total: Option<u64>,
    message: Option<String>,
    quiet: bool,
) -> Result<()> {
    let job_id_arg = job_id_str
        .filter(|job| !job.trim().is_empty())
        .or_else(|| {
            std::env::var("GFLOW_JOB_ID")
                .ok()
                .filter(|id| !id.trim().is_empty())
        })
        .unwrap_or_else(|| "@".to_string());

    let client = gflow::create_client(config_path)?;
    let job_id = crate::multicall::gjob::utils::resolve_job_id(&client, job_id_arg.trim()).await?;

    if let Some(total) = total {
        if value > total && !quiet {
            eprintln!(
                "Warning: value {} exceeds total {}; the reported percent is clamped to 100%.",
                value, total
            );
        }
    }

    let progress = JobProgress {
        job_id: Some(job_id),
        value,
        total,
        message: message.and_then(|m| {
            let trimmed = m.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        }),
        ..JobProgress::default()
    };

    client.set_job_progress(job_id, &progress).await?;

    if !quiet {
        println!("Progress for job {}: {}", job_id, describe(&progress));
    }

    Ok(())
}

fn describe(progress: &JobProgress) -> String {
    match (progress.total, progress.message.as_deref()) {
        (Some(total), Some(message)) => format!("{}/{} ({})", progress.value, total, message),
        (Some(total), None) => format!("{}/{}", progress.value, total),
        (None, Some(message)) => format!("{} ({})", progress.value, message),
        (None, None) => progress.value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_documents_with_and_without_a_total() {
        let with_both = JobProgress {
            value: 25,
            total: Some(40),
            message: Some("epoch 25".into()),
            ..JobProgress::default()
        };
        assert_eq!(describe(&with_both), "25/40 (epoch 25)");

        let bare = JobProgress {
            value: 7,
            ..JobProgress::default()
        };
        assert_eq!(describe(&bare), "7");
    }
}
