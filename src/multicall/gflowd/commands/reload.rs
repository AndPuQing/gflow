use anyhow::{anyhow, Result};
use gflow::tmux::TmuxSession;
use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;
use tmux_interface::{ListPanes, RenameSession, Tmux};

pub async fn handle_reload(
    config_path: &Option<std::path::PathBuf>,
    gpus: Option<String>,
) -> Result<()> {
    // Load config to get daemon URL
    let config = gflow::config::load_config(config_path.as_ref()).unwrap_or_default();
    let client = gflow::Client::build(&config)?;

    // 1. Check if daemon is running
    let pid = get_daemon_pid().await?;
    tracing::info!("Found running daemon at PID {}", pid);
    let mut old_pids: HashSet<u32> = pgrep_gflowd_pids()
        .unwrap_or_default()
        .into_iter()
        .collect();
    old_pids.insert(pid);

    // 2. Start new daemon instance in temporary tmux session
    println!("Starting new daemon instance...");
    tracing::info!("Starting new daemon instance...");
    let new_session_name = format!("gflow_server_new_{}", std::process::id());
    let session = TmuxSession::new(new_session_name.clone());

    let mut command = String::from("gflowd -vvv");
    if let Some(gpu_spec) = gpus {
        command.push_str(&format!(" --gpus-internal '{}'", gpu_spec));
    }

    session.send_command(&command);

    // Enable pipe-pane to capture daemon logs to file
    if let Ok(log_path) = gflow::core::get_daemon_log_file_path() {
        if let Err(e) = session.enable_pipe_pane(&log_path) {
            tracing::warn!("Failed to enable daemon log capture: {}", e);
        }
    }

    // 3. Wait for new instance to initialize and bind socket
    tracing::info!("Waiting for new instance to initialize...");
    tokio::time::sleep(Duration::from_millis(250)).await;

    // 4. Verify new instance is running by checking the tmux session directly
    // NOTE: We cannot rely on HTTP health checks with SO_REUSEPORT because
    // the kernel load-balances requests between old and new daemon, making
    // it unreliable to detect the new instance via HTTP.
    tracing::info!(
        "Verifying new daemon instance (distinct from old PID {})...",
        pid
    );

    let new_pid = match wait_for_new_daemon_pid(&new_session_name, pid, &old_pids).await {
        Ok(new_pid) => {
            tracing::info!("Confirmed new daemon instance at PID {}", new_pid);
            new_pid
        }
        Err(e) => {
            tracing::error!("Failed to get new daemon PID: {}", e);
            gflow::tmux::kill_session(&new_session_name).ok();
            return Err(anyhow!("Could not verify new daemon instance: {}", e));
        }
    };

    if !is_process_running(new_pid) {
        gflow::tmux::kill_session(&new_session_name).ok();
        return Err(anyhow!(
            "New daemon process (PID {}) exited immediately after startup",
            new_pid
        ));
    }

    // 5. Verify the new daemon is responsive (make a few health check attempts)
    // This is a best-effort check - we already know the daemon process exists
    println!("Verifying new daemon...");
    tracing::info!("Checking new daemon responsiveness...");
    let mut health_check_passed = false;
    for attempt in 1..=10 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if !is_process_running(new_pid) {
            gflow::tmux::kill_session(&new_session_name).ok();
            return Err(anyhow!(
                "New daemon process (PID {}) exited during health checks",
                new_pid
            ));
        }
        if let Ok(Some(health_pid)) = client.get_health_with_pid().await {
            if health_pid == new_pid {
                tracing::info!(
                    "New daemon is responsive (health check returned PID {}, attempt {})",
                    health_pid,
                    attempt
                );
                health_check_passed = true;
                break;
            }
        }
    }

    if !health_check_passed {
        tracing::warn!(
            "Could not confirm new daemon responsiveness via health checks, \
             but process exists at PID {}. Continuing with reload...",
            new_pid
        );
    }

    // 6. Signal old process to shutdown (SIGUSR2)
    println!("Switching to new daemon...");
    tracing::info!("Signaling old daemon (PID {}) to shutdown", pid);
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGUSR2);
    }

    // 7. Wait for old process to exit
    let mut exited = false;
    for i in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !is_process_running(pid) {
            tracing::info!("Old daemon has exited");
            exited = true;
            break;
        }
        if i == 29 {
            tracing::warn!(
                "Old daemon did not exit within 3 seconds. \
                 New daemon is running, but old process may need manual cleanup."
            );
        }
    }

    // 8. Rename new tmux session to standard name
    // First, ensure old session is gone
    gflow::tmux::kill_session(super::TMUX_SESSION_NAME).ok();

    let rename_result = Tmux::with_command(
        RenameSession::new()
            .target_session(&new_session_name)
            .new_name(super::TMUX_SESSION_NAME),
    )
    .output();

    match rename_result {
        Ok(output) if output.success() => {
            println!("gflowd reloaded successfully.");
            if !exited {
                println!(
                    "Note: Old daemon process (PID {}) may still be running. \
                     You can manually check with 'ps -p {}'",
                    pid, pid
                );
            }
            Ok(())
        }
        Ok(_) => Err(anyhow!(
            "Failed to rename new session. \
                 New daemon is running as session '{}', you may need to rename it manually.",
            new_session_name
        )),
        Err(e) => Err(anyhow!(
            "Failed to execute tmux rename: {}. \
             New daemon is running as session '{}', you may need to rename it manually.",
            e,
            new_session_name
        )),
    }
}

async fn get_daemon_pid() -> Result<u32> {
    // Strategy: Find gflowd process that is a descendant of the gflow_server tmux session
    // This is more reliable than just pgrep when multiple daemons might be running

    // First, verify tmux session exists
    if !gflow::tmux::is_session_exist(super::TMUX_SESSION_NAME) {
        return Err(anyhow!(
            "gflowd tmux session '{}' not found. Is the daemon running?",
            super::TMUX_SESSION_NAME
        ));
    }

    let (pane_pid, pane_cmd) = tmux_pane_pid_and_current_command(super::TMUX_SESSION_NAME)?;
    if pane_cmd == "gflowd" {
        return Ok(pane_pid);
    }

    // pane_pid is the shell when gflowd isn't in the foreground.
    let shell_pid = pane_pid;
    let pids = pgrep_gflowd_pids()?;

    // For each candidate PID, check if it's a child of the shell PID
    for pid in &pids {
        if is_process_descendant_of(*pid, shell_pid) {
            tracing::debug!(
                "Found gflowd PID {} as descendant of tmux session (shell PID {})",
                pid,
                shell_pid
            );
            return Ok(*pid);
        }
    }

    // Fallback: if no descendant found, use the first PID (for backward compatibility)
    tracing::warn!(
        "Could not verify gflowd PID via tmux session ancestry, using first match: {}",
        pids[0]
    );
    Ok(pids[0])
}

fn is_process_descendant_of(pid: u32, ancestor_pid: u32) -> bool {
    let mut current_pid = pid;

    // Walk up the process tree up to 10 levels
    for _ in 0..10 {
        if current_pid == ancestor_pid {
            return true;
        }

        // Get parent PID from /proc/<pid>/stat
        let stat_path = format!("/proc/{}/stat", current_pid);
        if let Ok(stat) = std::fs::read_to_string(&stat_path) {
            if let Some((ppid, _start_time)) = parse_proc_stat_ppid_and_starttime(&stat) {
                if ppid <= 1 {
                    break; // Reached init
                }
                current_pid = ppid;
                continue;
            }
        }
        break;
    }

    false
}

fn is_process_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

async fn wait_for_new_daemon_pid(
    session_name: &str,
    old_pid: u32,
    old_pids: &HashSet<u32>,
) -> Result<u32> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut last_tmux_err: Option<anyhow::Error> = None;

    while tokio::time::Instant::now() < deadline {
        // Fast path: if gflowd is running in the foreground in this tmux pane, pane_pid is the daemon PID.
        match tmux_pane_pid_and_current_command(session_name) {
            Ok((pane_pid, cmd)) => {
                if cmd == "gflowd" && pane_pid != old_pid {
                    return Ok(pane_pid);
                }
            }
            Err(e) => last_tmux_err = Some(e),
        }

        // Fallback: detect a newly-created gflowd PID by diffing process lists.
        if let Ok(pids) = pgrep_gflowd_pids() {
            let mut new_candidates: Vec<u32> = pids
                .into_iter()
                .filter(|pid| *pid != old_pid && !old_pids.contains(pid))
                .collect();

            if !new_candidates.is_empty() {
                if new_candidates.len() == 1 {
                    return Ok(new_candidates[0]);
                }

                new_candidates.sort_by_key(|pid| proc_start_time(*pid).unwrap_or(0));
                return Ok(*new_candidates.last().unwrap());
            }
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    Err(anyhow!(
        "Timed out waiting for new gflowd process{}",
        last_tmux_err
            .as_ref()
            .map(|e| format!(" (last tmux error: {})", e))
            .unwrap_or_default()
    ))
}

fn tmux_pane_pid_and_current_command(session_name: &str) -> Result<(u32, String)> {
    if !gflow::tmux::is_session_exist(session_name) {
        return Err(anyhow!("tmux session '{}' not found", session_name));
    }

    let output = Tmux::with_command(
        ListPanes::new()
            .target(session_name)
            .format("#{pane_pid}\t#{pane_current_command}"),
    )
    .output()?;

    if !output.success() {
        return Err(anyhow!(
            "Failed to get tmux pane info for session '{}'",
            session_name
        ));
    }

    let stdout = String::from_utf8(output.stdout().to_vec())?;
    let first_line = stdout.lines().next().unwrap_or("").trim();
    let (pid_str, cmd) = first_line
        .split_once('\t')
        .ok_or_else(|| anyhow!("Unexpected tmux pane output: '{}'", first_line))?;

    let pid = pid_str
        .trim()
        .parse::<u32>()
        .map_err(|e| anyhow!("Failed to parse pane PID: {}", e))?;

    Ok((pid, cmd.trim().to_string()))
}

fn pgrep_gflowd_pids() -> Result<Vec<u32>> {
    let uid = unsafe { libc::getuid() }.to_string();

    let output = Command::new("pgrep")
        .args(["-u", &uid, "-x", "gflowd"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "gflowd daemon process not found (tried pgrep). Is the daemon running?"
        ));
    }

    let stdout = String::from_utf8(output.stdout)?;
    let pids: Vec<u32> = stdout
        .trim()
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect();

    if pids.is_empty() {
        return Err(anyhow!("No gflowd daemon process found"));
    }

    Ok(pids)
}

fn proc_start_time(pid: u32) -> Option<u64> {
    let stat_path = format!("/proc/{}/stat", pid);
    let stat = std::fs::read_to_string(&stat_path).ok()?;
    let (_, start_time) = parse_proc_stat_ppid_and_starttime(&stat)?;
    Some(start_time)
}

fn parse_proc_stat_ppid_and_starttime(stat: &str) -> Option<(u32, u64)> {
    // /proc/<pid>/stat is: pid (comm) state ppid ... starttime ...
    // comm can contain spaces, so split at the last ')'.
    let end = stat.rfind(')')?;
    let after = stat.get(end + 1..)?.trim_start();
    let mut it = after.split_whitespace();

    // state (field 3)
    let _state = it.next()?;

    // ppid (field 4)
    let ppid: u32 = it.next()?.parse().ok()?;

    // starttime is field 22 => index 19 in tokens after comm (including state).
    // We already consumed state and ppid, so we need the 18th remaining token.
    let tokens: Vec<&str> = it.collect();
    if tokens.len() < 18 {
        return None;
    }
    let start_time: u64 = tokens.get(17)?.parse().ok()?;
    Some((ppid, start_time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proc_stat_handles_comm_and_extracts_fields() {
        // Example based on procfs format; comm may include spaces.
        let stat = "12345 (gflowd worker) S 111 222 333 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 987654 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let (ppid, start) = parse_proc_stat_ppid_and_starttime(stat).unwrap();
        assert_eq!(ppid, 111);
        assert_eq!(start, 987654);
    }
}
