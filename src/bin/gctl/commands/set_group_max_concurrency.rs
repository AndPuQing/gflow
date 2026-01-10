use anyhow::{Context, Result};
use gflow::client::Client;

pub async fn handle_set_group_max_concurrency(
    client: &Client,
    job_or_group_id: &str,
    max_concurrent: usize,
) -> Result<()> {
    // Try to parse as job ID (u32) first
    let group_id = if let Ok(job_id) = job_or_group_id.parse::<u32>() {
        // Look up the job to get its group_id
        let job = client
            .get_job(job_id)
            .await
            .context(format!("Failed to fetch job {}", job_id))?
            .ok_or_else(|| anyhow::anyhow!("Job {} not found", job_id))?;

        let group_id = job
            .group_id
            .ok_or_else(|| anyhow::anyhow!("Job {} is not part of a group", job_id))?;

        println!("Found job {} in group '{}'", job_id, group_id);
        group_id
    } else {
        // Assume it's a group_id (UUID)
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
