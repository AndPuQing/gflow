use anyhow::Result;
use gflow::client::Client;

pub async fn handle_release(config_path: &Option<std::path::PathBuf>, job_id: u32) -> Result<()> {
    // Load config and create client
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    // Get the job from the daemon to check its state
    let job = client.get_job(job_id).await?;

    let job = match job {
        Some(job) => job,
        None => {
            eprintln!("Error: Job {} not found", job_id);
            return Ok(());
        }
    };

    // Check if the job can be released
    if job.state != gflow::core::job::JobState::Hold {
        eprintln!(
            "Error: Job {} is in state '{}' and cannot be released. Only held jobs can be released.",
            job_id, job.state
        );
        return Ok(());
    }

    // Release the job
    client.release_job(job_id).await?;
    println!("Job {} released back to queue.", job_id);

    Ok(())
}
