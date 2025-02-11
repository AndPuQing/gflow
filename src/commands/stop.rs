use anyhow::{Context, Result};
use std::process::Command;

pub(crate) fn handle_stop() -> Result<()> {
    log::debug!("Stopping the system service");

    let output = Command::new("systemctl")
        .arg("stop")
        .arg("gflowd")
        .output()
        .context("Failed to execute systemctl command")?;

    if output.status.success() {
        log::info!("Service stopped successfully");
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to stop service: {}", error)
    }
}
