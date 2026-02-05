use anyhow::Result;
use gflow::utils::parse_job_ids;

pub async fn handle_release(
    config_path: &Option<std::path::PathBuf>,
    job_ids_str: String,
) -> Result<()> {
    let client = gflow::create_client(config_path)?;

    let job_ids = parse_job_ids(&job_ids_str)?;

    for &job_id in &job_ids {
        // Get the job from the daemon to check its state
        let Some(job) = gflow::client::get_job_or_warn(&client, job_id).await? else {
            continue;
        };

        // Check if the job can be released
        if let Err(e) =
            gflow::utils::validate_job_state(&job, gflow::core::job::JobState::Hold, "released")
        {
            eprintln!("Error: {}", e);
            continue;
        }

        // Release the job
        client.release_job(job_id).await?;
        println!("Job {} released back to queue.", job_id);
    }

    Ok(())
}
