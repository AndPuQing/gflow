use anyhow::{Context, Result};
use gflow::{random_run_name, Job};
use tmux_interface::{NewSession, SendKeys, Tmux};

struct TmuxSession {
    name: String,
}

impl TmuxSession {
    fn new(name: String) -> Result<Self> {
        Tmux::new()
            .add_command(NewSession::new().detached().session_name(&name))
            .output()
            .context("Failed to create tmux session")?;

        // Allow tmux session to initialize
        std::thread::sleep(std::time::Duration::from_secs(1));

        Ok(Self { name })
    }

    fn send_command(&self, command: &str) -> Result<()> {
        Tmux::new()
            .add_command(SendKeys::new().target_client(&self.name).key(command))
            .add_command(SendKeys::new().target_client(&self.name).key("Enter"))
            .output()
            .context(format!("Failed to send command: {}", command))?;

        Ok(())
    }
}

pub fn execute_job(job: &mut Job, gpu_slots: &[u32]) -> Result<()> {
    // Create tmux session
    let session = TmuxSession::new(random_run_name()).context("Failed to create tmux session")?;

    job.run_name = Some(session.name.clone());

    // Set GPU environment if needed
    if !gpu_slots.is_empty() {
        let cuda_devices = gpu_slots
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        session
            .send_command(&format!("export CUDA_VISIBLE_DEVICES={}", cuda_devices))
            .context("Failed to set CUDA_VISIBLE_DEVICES")?;
    }

    // Activate conda environment if specified
    if let Some(env) = &job.conda_env {
        session
            .send_command(&format!("conda activate {}", env))
            .context("Failed to activate conda environment")?;
    }

    // Execute the job command
    let command = if let Some(script) = &job.script {
        format!("sh {}", script.display())
    } else if let Some(cmd) = &job.command {
        cmd.clone()
    } else {
        anyhow::bail!("No command or script specified");
    };

    session
        .send_command(&command)
        .context("Failed to execute job command")?;

    Ok(())
}
