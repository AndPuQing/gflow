use std::path::Path;
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

    /// Enable pipe-pane to capture output to a log file
    pub fn enable_pipe_pane(&self, log_path: &Path) -> anyhow::Result<()> {
        let log_path_str = log_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid log path"))?;

        Tmux::with_command(
            tmux_interface::PipePane::new()
                .target_pane(&self.name)
                .open()
                .shell_command(format!("cat >> {}", log_path_str)),
        )
        .output()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Failed to enable pipe-pane: {}", e))
    }

    /// Disable pipe-pane for the session
    pub fn disable_pipe_pane(&self) -> anyhow::Result<()> {
        Tmux::with_command(tmux_interface::PipePane::new().target_pane(&self.name))
            .output()
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("Failed to disable pipe-pane: {}", e))
    }

    /// Check if pipe-pane is active for the session
    pub fn is_pipe_pane_active(&self) -> bool {
        Tmux::with_command(
            tmux_interface::DisplayMessage::new()
                .target_pane(&self.name)
                .print()
                .message("#{pane_pipe}"),
        )
        .output()
        .map(|output| output.success())
        .unwrap_or(false)
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
    // Disable pipe-pane before killing session (ignore errors if already disabled)
    Tmux::with_command(tmux_interface::PipePane::new().target_pane(name))
        .output()
        .ok();

    std::thread::sleep(std::time::Duration::from_secs(1));

    Tmux::with_command(tmux_interface::KillSession::new().target_session(name))
        .output()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Failed to kill tmux session: {}", e))
}

pub fn attach_to_session(name: &str) -> anyhow::Result<()> {
    // Check if session exists before attaching
    if !is_session_exist(name) {
        return Err(anyhow::anyhow!("Tmux session '{}' does not exist", name));
    }
    Tmux::with_command(tmux_interface::AttachSession::new().target_session(name))
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to attach to tmux session: {}", e))?;

    Ok(())
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
