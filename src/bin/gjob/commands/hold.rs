use anyhow::Result;
use gflow::client::Client;

pub async fn handle_hold(config_path: &Option<std::path::PathBuf>, job_id: u32) -> Result<()> {
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

    // Check if the job can be held
    if job.state != gflow::core::job::JobState::Queued {
        eprintln!(
            "Error: Job {} is in state '{}' and cannot be held. Only queued jobs can be held.",
            job_id, job.state
        );
        return Ok(());
    }

    // Hold the job
    client.hold_job(job_id).await?;
    println!("Job {} put on hold.", job_id);

    Ok(())
}
