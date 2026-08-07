use anyhow::{Context, Result};
use gflow::client::Client;

fn looks_like_job_selection(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_err() && (value.contains(',') || value.contains('-'))
}

pub async fn handle_set_group_max_concurrency(
    client: &Client,
    job_or_group_id: &str,
    max_concurrent: usize,
) -> Result<()> {
    // UUIDs contain hyphens too, so recognize an existing group ID before
    // treating a hyphenated argument as a Job ID range.
    if looks_like_job_selection(job_or_group_id) {
        let job_ids =
            gflow::utils::parse_job_ids(job_or_group_id).context("Invalid Job ID list or range")?;
        let (group_id, updated_jobs) = client
            .set_jobs_max_concurrency(&job_ids, max_concurrent)
            .await?;
        println!(
            "Set max_concurrent to {} for jobs {} (temporary group '{}', {} jobs affected)",
            max_concurrent, job_or_group_id, group_id, updated_jobs
        );
        return Ok(());
    }

    // Try to parse as one job ID (u32) first. A job in an existing group keeps
    // the legacy group-wide behavior; an independent job gets a temporary group.
    let group_id = if let Ok(job_id) = job_or_group_id.parse::<u32>() {
        let job = client
            .get_job(job_id)
            .await
            .context(format!("Failed to fetch job {}", job_id))?
            .ok_or_else(|| anyhow::anyhow!("Job {} not found", job_id))?;

        if let Some(group_uuid) = job.group_id {
            let group_id = group_uuid.to_string();
            println!("Found job {} in group '{}'", job_id, group_id);
            group_id
        } else {
            let (group_id, updated_jobs) = client
                .set_jobs_max_concurrency(&[job_id], max_concurrent)
                .await?;
            println!(
                "Set max_concurrent to {} for job {} (temporary group '{}', {} jobs affected)",
                max_concurrent, job_id, group_id, updated_jobs
            );
            return Ok(());
        }
    } else {
        // Preserve the existing group ID path; the server validates the UUID.
        job_or_group_id.to_string()
    };

    let updated_jobs = client
        .set_group_max_concurrency(&group_id, max_concurrent)
        .await?;

    println!(
        "Updated max_concurrency to {} for group '{}' ({} jobs affected)",
        max_concurrent, group_id, updated_jobs
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::looks_like_job_selection;

    #[test]
    fn recognizes_job_lists_and_ranges() {
        assert!(looks_like_job_selection("1,2,5"));
        assert!(looks_like_job_selection("10-20"));
        assert!(looks_like_job_selection("1-3,8"));
    }

    #[test]
    fn keeps_uuid_group_ids_on_the_legacy_path() {
        assert!(!looks_like_job_selection(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
    }
}
