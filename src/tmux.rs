use std::path::Path;
use tmux_interface::{KillSession, NewSession, PipePane, SendKeys, Tmux};

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

/// Get all existing tmux session names in a single call
/// This is much more efficient than checking each session individually
pub fn get_all_session_names() -> std::collections::HashSet<String> {
    Tmux::with_command(tmux_interface::ListSessions::new().format("#{session_name}"))
        .output()
        .map(|output| {
            if output.success() {
                let stdout_bytes = output.stdout();
                let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                stdout_str.lines().map(|line| line.to_string()).collect()
            } else {
                std::collections::HashSet::new()
            }
        })
        .unwrap_or_else(|_| std::collections::HashSet::new())
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

/// Kill multiple tmux sessions in batch using a single tmux command
/// This is much faster than killing sessions sequentially
/// Returns a vector of tuples: (session_name, result)
pub fn kill_sessions_batch(names: &[String]) -> Vec<(String, anyhow::Result<()>)> {
    if names.is_empty() {
        return Vec::new();
    }

    // Get all existing sessions to filter out non-existent ones
    let existing_sessions = get_all_session_names();

    // Separate existing and non-existing sessions
    let (existing, non_existing): (Vec<_>, Vec<_>) = names
        .iter()
        .partition(|name| existing_sessions.contains(*name));

    let mut results = Vec::new();

    // Add results for non-existing sessions
    for name in non_existing {
        results.push((
            name.clone(),
            Err(anyhow::anyhow!("Session '{}' does not exist", name)),
        ));
    }

    // If no existing sessions, return early
    if existing.is_empty() {
        return results;
    }

    // Build a single tmux command with multiple pipe-pane disables and kill-session commands
    let mut tmux = Tmux::new();
    for name in &existing {
        tmux = tmux
            // Disable pipe-pane first (ignore errors if already disabled)
            .add_command(PipePane::new().target_pane(name.as_str()))
            // Kill the session
            .add_command(KillSession::new().target_session(name.as_str()));
    }

    // Execute all commands in a single tmux invocation
    let batch_result = tmux.output();

    // Map results back to individual sessions
    // Note: If the batch command succeeds, all sessions were killed successfully
    // If it fails, we can't easily determine which specific session failed in a batch operation
    match batch_result {
        Ok(_) => {
            for name in existing {
                results.push((name.clone(), Ok(())));
            }
        }
        Err(_) => {
            // If batch fails, fall back to individual kills to get granular error info
            for name in existing {
                let result = kill_session(name);
                results.push((name.clone(), result));
            }
        }
    }

    results
}

pub fn attach_to_session(name: &str) -> anyhow::Result<()> {
    Tmux::with_command(tmux_interface::AttachSession::new().target_session(name))
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to attach to tmux session: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tmux_interface::{HasSession, KillSession, Tmux};

    use super::*;

    #[test]
    fn test_tmux_session() {
        // Skip test if tmux is not usable (not just installed, but actually functional).
        // `tmux start-server` will fail in sandboxes where tmux can't connect/spawn.
        let tmux_usable = Command::new("tmux")
            .arg("start-server")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !tmux_usable {
            eprintln!(
                "Skipping test_tmux_session: tmux not usable (not installed or can't connect)"
            );
            return;
        }

        let session_name = format!(
            "gflow-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        TmuxSession::new(session_name.clone());
        let has_session = Tmux::with_command(HasSession::new().target_session(&session_name))
            .output()
            .unwrap();

        assert!(has_session.success());

        Tmux::with_command(KillSession::new().target_session(&session_name))
            .output()
            .unwrap();
    }
}
