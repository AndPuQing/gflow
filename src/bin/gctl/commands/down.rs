use anyhow::Result;
use tmux_interface::{KillSession, Tmux};

pub async fn handle_down() -> Result<()> {
    if let Err(e) =
        Tmux::with_command(KillSession::new().target_session(super::TMUX_SESSION_NAME)).output()
    {
        eprintln!("Failed to stop gflowd: {e}");
    } else {
        println!("gflowd stopped.");
    }
    Ok(())
}
