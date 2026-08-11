use anyhow::Result;
use gflow::utils::parse_job_ids;

pub async fn handle_release(
    config_path: &Option<std::path::PathBuf>,
    job_ids_str: String,
    at: Option<String>,
) -> Result<()> {
    let client = gflow::create_client(config_path)?;

    // Parse the delayed-release time (if any) up front so a malformed value
    // fails before any job is touched.
    let scheduled_at = match &at {
        Some(time_str) => Some(gflow::utils::parse_begin_time(time_str)?),
        None => None,
    };

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

        // Release the job (immediately, or deferred to the given time)
        match scheduled_at {
            Some(at) => {
                client.release_job_at(job_id, at).await?;
                println!(
                    "Job {} released back to queue (scheduled to start at {}).",
                    job_id,
                    gflow::utils::format_system_time(at)
                );
            }
            None => {
                client.release_job(job_id).await?;
                println!("Job {} released back to queue.", job_id);
            }
        }
    }

    Ok(())
}
