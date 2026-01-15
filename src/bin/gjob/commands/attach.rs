use anyhow::Result;
use gflow::{
    client::Client,
    tmux::{attach_to_session, is_session_exist},
};

pub async fn handle_attach(
    config_path: &Option<std::path::PathBuf>,
    job_id_str: &str,
) -> Result<()> {
    // Load config and create client
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    // Resolve job ID (handle @ shorthand)
    let job_id = crate::utils::resolve_job_id(&client, job_id_str).await?;

    // Get the job from the daemon
    let job = client.get_job(job_id).await?;

    let job = match job {
        Some(job) => job,
        None => {
            eprintln!("Error: Job {} not found", job_id);
            return Ok(());
        }
    };

    // Check if the job has a tmux session
    let session_name = match job.run_name {
        Some(ref name) => name,
        None => {
            eprintln!(
                "Error: Job {} does not have an associated tmux session",
                job_id
            );
            return Ok(());
        }
    };

    // Check if the tmux session exists
    if !is_session_exist(session_name) {
        eprintln!(
            "Error: Tmux session '{}' for job {} does not exist",
            session_name, job_id
        );
        return Ok(());
    }

    // Attach to the tmux session
    println!(
        "Attaching to tmux session '{}' for job {}...",
        session_name, job_id
    );
    attach_to_session(session_name)?;

    Ok(())
}
