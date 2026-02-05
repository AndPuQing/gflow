use anyhow::{Context, Result};
use gflow::{client::Client, core::job::JobState, utils::parse_job_ids};

pub async fn handle_cancel(client: &Client, ids: &str, dry_run: bool) -> Result<()> {
    let job_ids = parse_job_ids(ids)?;

    if dry_run {
        perform_dry_run(client, &job_ids).await?;
    } else {
        for job_id in &job_ids {
            client.cancel_job(*job_id).await?;
            println!("Job {} cancelled.", job_id);
        }
    }

    Ok(())
}

async fn perform_dry_run(client: &Client, job_ids: &[u32]) -> Result<()> {
    for &job_id in job_ids {
        let job = client
            .get_job(job_id)
            .await?
            .context(format!("Job {} not found", job_id))?;

        let can_cancel = job.state.can_transition_to(JobState::Cancelled);

        if can_cancel {
            println!("{}\t{}\tok", job_id, job.state);
        } else {
            println!("{}\t{}\tinvalid", job_id, job.state);
            continue;
        }

        // Find dependent jobs that would be affected
        let all_jobs = client.list_jobs().await?;
        for dep_job in all_jobs
            .iter()
            .filter(|j| j.depends_on == Some(job_id))
            .filter(|j| matches!(j.state, JobState::Queued | JobState::Hold))
        {
            println!("{}\t{}\tblocked\t{}", dep_job.id, dep_job.state, job_id);
        }
    }

    Ok(())
}
