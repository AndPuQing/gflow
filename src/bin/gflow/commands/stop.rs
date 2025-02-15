use anyhow::Result;
use tmux_interface::{KillSession, Tmux};

use crate::commands::start::TMUX_SESSION_NAME;

pub(crate) fn handle_stop() -> Result<()> {
    log::debug!("Stopping the system service");
    Tmux::with_command(KillSession::new().target_session(TMUX_SESSION_NAME))
        .output()
        .unwrap();
    Ok(())
}
