use anyhow::Result;
use gflow::tmux::TmuxSession;

pub async fn handle_up(gpus: Option<String>) -> Result<()> {
    let session = TmuxSession::new(super::TMUX_SESSION_NAME.to_string());

    let mut command = String::from("gflowd -vvv");
    if let Some(gpu_spec) = gpus {
        command.push_str(&format!(" --gpus-internal '{}'", gpu_spec));
    }

    session.send_command(&command);

    // Enable pipe-pane to capture daemon logs to file
    if let Ok(log_path) = gflow::core::get_daemon_log_file_path() {
        if let Err(e) = session.enable_pipe_pane(&log_path) {
            eprintln!("Warning: Failed to enable daemon log capture: {}", e);
        }
    }

    println!("gflowd started.");
    Ok(())
}
