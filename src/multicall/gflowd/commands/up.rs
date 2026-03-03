use anyhow::Result;
use gflow::tmux::TmuxSession;

pub async fn handle_up(gpus: Option<String>) -> Result<()> {
    let session = TmuxSession::new(super::TMUX_SESSION_NAME.to_string());
    let command = super::daemon_start_command(gpus.as_deref())?;

    session.send_command(&command);

    println!("gflowd started.");
    Ok(())
}
