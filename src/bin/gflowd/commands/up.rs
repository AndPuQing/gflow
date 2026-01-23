use anyhow::Result;
use gflow::tmux::TmuxSession;

pub async fn handle_up(gpus: Option<String>) -> Result<()> {
    let session = TmuxSession::new(super::TMUX_SESSION_NAME.to_string());

    // Replay historical daemon logs to the tmux session
    // Note: We replay logs BEFORE enabling pipe-pane to avoid capturing
    // the cat output back into the log file (which would cause duplication)
    if let Ok(log_path) = gflow::core::get_daemon_log_file_path() {
        if let Err(e) = session.replay_log_file(&log_path) {
            eprintln!("Warning: Failed to replay daemon logs: {}", e);
        }
        // Wait for cat command to complete before sending next command
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let mut command = String::from("gflowd -vvv");
    if let Some(gpu_spec) = gpus {
        command.push_str(&format!(" --gpus-internal '{}'", gpu_spec));
    }

    session.send_command(&command);

    // Wait a bit for the daemon process to start before enabling pipe-pane
    // This ensures we capture the daemon output, not the command echo
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Enable pipe-pane to capture daemon logs to file
    if let Ok(log_path) = gflow::core::get_daemon_log_file_path() {
        if let Err(e) = session.enable_pipe_pane(&log_path) {
            eprintln!("Warning: Failed to enable daemon log capture: {}", e);
        }
    }

    println!("gflowd started.");
    Ok(())
}
