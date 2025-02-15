use tmux_interface::{NewSession, SendKeys, Tmux};

pub struct TmuxSession {
    pub name: String,
}

impl TmuxSession {
    pub fn new(name: String) -> Self {
        Tmux::new()
            .add_command(NewSession::new().detached().session_name(&name))
            .output()
            .unwrap();

        // Allow tmux session to initialize
        std::thread::sleep(std::time::Duration::from_secs(1));

        Self { name }
    }

    pub fn send_command(&self, command: &str) {
        Tmux::new()
            .add_command(SendKeys::new().target_pane(&self.name).key(command))
            .add_command(SendKeys::new().target_pane(&self.name).key("Enter"))
            .output()
            .unwrap();
    }
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
