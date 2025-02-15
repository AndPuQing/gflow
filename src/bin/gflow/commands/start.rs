use anyhow::Result;
use gflow::tmux::TmuxSession;

pub static TMUX_SESSION_NAME: &str = "gflow_server";

pub(crate) fn handle_start() -> Result<()> {
    let session = TmuxSession::new(TMUX_SESSION_NAME.to_string());

    session.send_command("gflowd -vvv");
    log::info!("Started the system service");
    Ok(())
}
