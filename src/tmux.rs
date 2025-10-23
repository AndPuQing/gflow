use tmux_interface::{NewSession, SendKeys, Tmux};

/// A tmux session
pub struct TmuxSession {
    pub name: String, // Name of the tmux session
}

impl TmuxSession {
    /// Create a new tmux session with the given name
    pub fn new(name: String) -> Self {
        Tmux::new()
            .add_command(NewSession::new().detached().session_name(&name))
            .output()
            .ok();

        // Allow tmux session to initialize
        std::thread::sleep(std::time::Duration::from_secs(1));

        Self { name }
    }

    /// Send a command to the tmux session
    pub fn send_command(&self, command: &str) {
        Tmux::new()
            .add_command(SendKeys::new().target_pane(&self.name).key(command))
            .add_command(SendKeys::new().target_pane(&self.name).key("Enter"))
            .output()
            .ok();
    }
}

pub fn is_session_exist(name: &str) -> bool {
    Tmux::with_command(tmux_interface::HasSession::new().target_session(name))
        .output()
        .map(|output| output.success())
        .unwrap_or(false)
}

pub fn send_ctrl_c(name: &str) -> anyhow::Result<()> {
    Tmux::with_command(SendKeys::new().target_pane(name).key("C-c"))
        .output()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Failed to send C-c to tmux session: {}", e))
}

pub fn kill_session(name: &str) -> anyhow::Result<()> {
    Tmux::with_command(tmux_interface::KillSession::new().target_session(name))
        .output()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Failed to kill tmux session: {}", e))
}

#[cfg(test)]
mod tests {
    use tmux_interface::{HasSession, KillSession, Tmux};

    use super::*;

    #[test]
    fn test_tmux_session() {
        TmuxSession::new("test".to_string());
        let has_session = Tmux::with_command(HasSession::new().target_session("test"))
            .output()
            .unwrap();

        assert!(has_session.success());

        Tmux::with_command(KillSession::new().target_session("test"))
            .output()
            .unwrap();
    }
}
