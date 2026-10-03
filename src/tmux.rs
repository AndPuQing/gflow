use std::path::Path;
use tmux_interface::{KillSession, NewSession, PipePane, SendKeys, Tmux};

/// A tmux session
pub struct TmuxSession {
    pub name: String, // Name of the tmux session
}

impl TmuxSession {
    /// Create a new tmux session with the given name
    pub fn new(name: String) -> Self {
        Self::create(name.clone()).unwrap_or(Self { name })
    }

    /// Create a new tmux session with the given name and surface tmux errors.
    pub fn create(name: String) -> anyhow::Result<Self> {
        let output = Tmux::new()
            .add_command(NewSession::new().detached().session_name(&name))
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to create tmux session '{}': {}", name, e))?;

        if !output.success() {
            let stderr = String::from_utf8_lossy(&output.stderr()).trim().to_string();
            anyhow::bail!(
                "Failed to create tmux session '{}': {}",
                name,
                if stderr.is_empty() {
                    "tmux returned a non-zero exit status"
                } else {
                    &stderr
                }
            );
        }

        // Allow tmux session to initialize
        std::thread::sleep(std::time::Duration::from_secs(1));

        Ok(Self { name })
    }

    /// Send a command to the tmux session
    pub fn send_command(&self, command: &str) {
        self.try_send_command(command).ok();
    }

    /// Send a command to the tmux session and surface tmux errors.
    pub fn try_send_command(&self, command: &str) -> anyhow::Result<()> {
        let output = Tmux::new()
            .add_command(SendKeys::new().target_pane(&self.name).key(command))
            .add_command(SendKeys::new().target_pane(&self.name).key("Enter"))
            .output()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to send command to tmux session '{}': {}",
                    self.name,
                    e
                )
            })?;

        if !output.success() {
            let stderr = String::from_utf8_lossy(&output.stderr()).trim().to_string();
            anyhow::bail!(
                "Failed to send command to tmux session '{}': {}",
                self.name,
                if stderr.is_empty() {
                    "tmux returned a non-zero exit status"
                } else {
                    &stderr
                }
            );
        }

        Ok(())
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

/// Normalize a user-provided session name into a tmux-safe identifier.
///
/// We keep letters, numbers, `-`, and `_`, and collapse all other separators
/// into `_` so the resulting name remains safe to pass back into tmux targets.
pub fn normalize_session_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut last_was_separator = false;

    for ch in name.trim().chars() {
        if ch.is_alphanumeric() || matches!(ch, '-' | '_') {
            normalized.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            normalized.push('_');
            last_was_separator = true;
        }
    }

    normalized.trim_matches('_').to_string()
}

pub fn is_session_exist(name: &str) -> bool {
    Tmux::with_command(tmux_interface::HasSession::new().target_session(name))
        .output()
        .map(|output| output.success())
        .unwrap_or(false)
}

/// Whether the tmux binary is installed and can be invoked.
///
/// When false, `gflowd up` falls back to hosting the daemon as a detached
/// process (no tmux required for job execution).
pub fn tmux_available() -> bool {
    std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
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

/// Disable pipe-pane for a session (standalone function)
/// This stops the `cat >> logfile` process without killing the tmux session
pub fn disable_pipe_pane(name: &str) -> anyhow::Result<()> {
    Tmux::with_command(tmux_interface::PipePane::new().target_pane(name))
        .output()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Failed to disable pipe-pane: {}", e))
}

/// Disable pipe-pane for a job's tmux session with appropriate logging.
/// Use `expect_failure=true` for cases where the session may already be gone (e.g., zombie jobs).
pub fn disable_pipe_pane_for_job(job_id: u32, session_name: &str, expect_failure: bool) {
    tracing::info!(
        "Disabling pipe-pane for job {} (session: {})",
        job_id,
        session_name
    );
    if let Err(e) = disable_pipe_pane(session_name) {
        if expect_failure {
            tracing::debug!(
                "Could not disable pipe-pane for session '{}' (may already be gone): {}",
                session_name,
                e
            );
        } else {
            tracing::warn!(
                "Failed to disable pipe-pane for session '{}': {}",
                session_name,
                e
            );
        }
    }
}

pub fn kill_session(name: &str) -> anyhow::Result<()> {
    // Disable pipe-pane before killing session (ignore errors if already disabled)
    Tmux::with_command(tmux_interface::PipePane::new().target_pane(name))
        .output()
        .ok();

    std::thread::sleep(std::time::Duration::from_secs(1));

    let output = Tmux::with_command(tmux_interface::KillSession::new().target_session(name))
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to kill tmux session '{}': {}", name, e))?;

    if !output.success() {
        let stderr = String::from_utf8_lossy(&output.stderr()).trim().to_string();
        anyhow::bail!(
            "Failed to kill tmux session '{}': {}",
            name,
            if stderr.is_empty() {
                "tmux returned a non-zero exit status"
            } else {
                &stderr
            }
        );
    }

    Ok(())
}

/// tmux rejects a command whose argv exceeds 1000 entries (`cmd_unpack_argv`
/// in tmux's `cmd.c` reports "command too long"), and the packed argv must
/// also fit into `MAX_IMSGSIZE` (16 KiB). Each session below costs
/// `pipe-pane -t <name> ; kill-session -t <name>`, i.e. 8 argv entries and
/// `35 + 2 * name.len()` bytes, so batches are chunked to stay under both
/// limits with headroom.
const MAX_BATCH_SESSIONS: usize = 100;
const MAX_BATCH_BYTES: usize = 12 * 1024;

/// Estimated tmux argv size of one `pipe-pane` + `kill-session` pair.
fn session_batch_cost(name: &str) -> usize {
    35 + 2 * name.len()
}

/// Split sessions into chunks that each fit into a single tmux invocation.
fn chunk_sessions<'a>(sessions: &[&'a String]) -> Vec<Vec<&'a String>> {
    let mut chunks = Vec::new();
    let mut current: Vec<&String> = Vec::new();
    let mut bytes = 0;

    for &name in sessions {
        let cost = session_batch_cost(name);
        if !current.is_empty()
            && (current.len() >= MAX_BATCH_SESSIONS || bytes + cost > MAX_BATCH_BYTES)
        {
            chunks.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes += cost;
        current.push(name);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Kill multiple tmux sessions in batched tmux invocations.
/// This is much faster than killing sessions sequentially.
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

    // A single oversized tmux command fails with "command too long" and kills
    // nothing, so split the sessions into chunks that fit tmux's limits.
    for chunk in chunk_sessions(&existing) {
        // Build one tmux command with pipe-pane disables and kill-session commands
        let mut tmux = Tmux::new();
        for name in &chunk {
            tmux = tmux
                // Disable pipe-pane first (ignore errors if already disabled)
                .add_command(PipePane::new().target_pane(name.as_str()))
                // Kill the session
                .add_command(KillSession::new().target_session(name.as_str()));
        }

        // Execute the whole chunk in a single tmux invocation
        let batch_succeeded = tmux
            .output()
            .map(|output| output.success())
            .unwrap_or(false);

        if batch_succeeded {
            for name in chunk {
                results.push((name.clone(), Ok(())));
            }
        } else {
            // tmux stops a command sequence at the first error, so we cannot
            // tell which sessions of the chunk were killed. Retry each one
            // individually to get per-session results.
            for name in chunk {
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
    use tmux_interface::{HasSession, KillSession, NewSession, Tmux};

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

    #[test]
    fn test_tmux_session_create_reports_duplicate_session() {
        let tmux_usable = Command::new("tmux")
            .arg("start-server")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !tmux_usable {
            eprintln!(
                "Skipping test_tmux_session_create_reports_duplicate_session: tmux not usable"
            );
            return;
        }

        let session_name = format!(
            "gflow-test-dup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        let _session = TmuxSession::create(session_name.clone()).unwrap();
        let error = TmuxSession::create(session_name.clone()).err().unwrap();

        assert!(error.to_string().contains("Failed to create tmux session"));

        Tmux::with_command(KillSession::new().target_session(&session_name))
            .output()
            .unwrap();
    }

    #[test]
    fn normalize_session_name_replaces_tmux_target_delimiters() {
        assert_eq!(
            normalize_session_name(" train:v1.2 / gpu#0 "),
            "train_v1_2_gpu_0"
        );
        assert_eq!(normalize_session_name("中文:实验.1"), "中文_实验_1");
        assert_eq!(normalize_session_name("___"), "");
    }

    #[test]
    fn test_kill_sessions_batch_over_tmux_argc_limit() {
        let tmux_usable = Command::new("tmux")
            .arg("start-server")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !tmux_usable {
            eprintln!("Skipping test_kill_sessions_batch_over_tmux_argc_limit: tmux not usable");
            return;
        }

        // 150 sessions exceed tmux's 1000-argv command limit in a single batch
        // (8 argv entries per session), so this exercises the chunking path.
        let prefix = format!(
            "gflow-test-batch-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let names: Vec<String> = (0..150).map(|i| format!("{}-{}", prefix, i)).collect();

        for name in &names {
            let output =
                Tmux::with_command(NewSession::new().detached().session_name(name.as_str()))
                    .output()
                    .unwrap();
            assert!(output.success(), "failed to create session '{}'", name);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));

        let results = kill_sessions_batch(&names);

        // Clean up any survivors before asserting so a failure does not leak
        // test sessions into the user's tmux server.
        let remaining = get_all_session_names();
        for name in &names {
            if remaining.contains(name) {
                let _ =
                    Tmux::with_command(KillSession::new().target_session(name.as_str())).output();
            }
        }

        assert_eq!(results.len(), names.len());
        for (name, result) in &results {
            assert!(result.is_ok(), "session '{}' failed: {:?}", name, result);
        }
        let remaining = get_all_session_names();
        for name in &names {
            assert!(
                !remaining.contains(name),
                "session '{}' was not killed",
                name
            );
        }
    }

    #[test]
    fn test_kill_session_reports_missing_session() {
        let tmux_usable = Command::new("tmux")
            .arg("start-server")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !tmux_usable {
            eprintln!("Skipping test_kill_session_reports_missing_session: tmux not usable");
            return;
        }

        let missing = format!(
            "gflow-test-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let error = kill_session(&missing).unwrap_err().to_string();
        assert!(
            error.contains("can't find session"),
            "unexpected error for missing session: {}",
            error
        );
    }
}
