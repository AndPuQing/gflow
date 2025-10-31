use anyhow::Result;
use gflow::tmux::TmuxSession;

pub async fn handle_up() -> Result<()> {
    let session = TmuxSession::new(super::TMUX_SESSION_NAME.to_string());
    session.send_command("gflowd -vvv");
    println!("gflowd started.");
    Ok(())
}
