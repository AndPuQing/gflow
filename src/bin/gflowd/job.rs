use anyhow::Result;
use gflow::{job::Job, random_run_name, tmux::TmuxSession};

pub fn execute_job(job: &mut Job) -> Result<()> {
    // Create tmux session
    let session = TmuxSession::new(random_run_name());

    job.run_name = Some(session.name.clone());
    let gpu_slots = job.gpu_ids.clone().unwrap();

    // Set run directory
    session.send_command(&format!("cd {}", job.run_dir.display()));

    // Set GPU environment if needed
    if !gpu_slots.is_empty() {
        let cuda_devices = gpu_slots
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        session.send_command(&format!("export CUDA_VISIBLE_DEVICES={}", cuda_devices));
    }

    // Activate conda environment if specified
    if let Some(env) = &job.conda_env {
        session.send_command(&format!("conda activate {}", env));
    }

    // Execute the job command
    let command = if let Some(script) = &job.script {
        format!("sh {} && gflow finish {}", script.display(), session.name)
    } else if let Some(cmd) = &job.command {
        format!("{} && gflow finish {}", cmd, session.name)
    } else {
        anyhow::bail!("No command or script specified");
    };

    session.send_command(&command);

    Ok(())
}
