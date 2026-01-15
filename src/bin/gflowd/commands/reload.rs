use anyhow::{anyhow, Result};
use gflow::tmux::TmuxSession;
use std::process::Command;
use std::time::Duration;
use tmux_interface::{ListPanes, RenameSession, Tmux};

pub async fn handle_reload(
    config_path: &Option<std::path::PathBuf>,
    gpus: Option<String>,
) -> Result<()> {
    // Load config to get daemon URL
    let config = gflow::config::load_config(config_path.as_ref()).unwrap_or_default();
    let client = gflow::client::Client::build(&config)?;

    // 1. Check if daemon is running
    let pid = get_daemon_pid().await?;
    tracing::info!("Found running daemon at PID {}", pid);

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

    // 3. Wait for new instance to initialize and bind socket
    tracing::info!("Waiting for new instance to initialize...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 4. Verify new instance is running by checking the tmux session directly
    // NOTE: We cannot rely on HTTP health checks with SO_REUSEPORT because
    // the kernel load-balances requests between old and new daemon, making
    // it unreliable to detect the new instance via HTTP.
    tracing::info!(
        "Verifying new daemon instance (distinct from old PID {})...",
        pid
    );

    let new_pid = match get_daemon_pid_from_session(&new_session_name).await {
        Ok(new_pid) if new_pid != pid => {
            tracing::info!("Confirmed new daemon instance at PID {}", new_pid);
            new_pid
        }
        Ok(same_pid) => {
            tracing::error!(
                "New daemon PID {} is the same as old daemon PID {}",
                same_pid,
                pid
            );
            gflow::tmux::kill_session(&new_session_name).ok();
            return Err(anyhow!(
                "New daemon PID matches old daemon PID. Reload failed."
            ));
        }
        Err(e) => {
            tracing::error!("Failed to get new daemon PID: {}", e);
            gflow::tmux::kill_session(&new_session_name).ok();
            return Err(anyhow!("Could not verify new daemon instance: {}", e));
        }
    };

    // 5. Verify the new daemon is responsive (make a few health check attempts)
    // This is a best-effort check - we already know the daemon process exists
    println!("Verifying new daemon...");
    tracing::info!("Checking new daemon responsiveness...");
    let mut health_check_passed = false;
    for attempt in 1..=10 {
        tokio::time::sleep(Duration::from_millis(300)).await;
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

    // Get the session's pane PID (this will be the shell)
    let pane_pid_output = Tmux::with_command(
        ListPanes::new()
            .target(super::TMUX_SESSION_NAME)
            .format("#{pane_pid}"),
    )
    .output()?;

    if !pane_pid_output.success() {
        return Err(anyhow!("Failed to get tmux pane PID"));
    }

    let shell_pid = String::from_utf8(pane_pid_output.stdout().to_vec())?
        .trim()
        .parse::<u32>()
        .map_err(|e| anyhow!("Failed to parse shell PID: {}", e))?;

    // Use pgrep to find gflowd processes
    let output = Command::new("pgrep")
        .args(["-f", "^gflowd -vvv"])
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

async fn get_daemon_pid_from_session(session_name: &str) -> Result<u32> {
    // Strategy: Find gflowd process that is a descendant of the specified tmux session
    // This allows us to find the daemon in the new temporary session

    // First, verify tmux session exists
    if !gflow::tmux::is_session_exist(session_name) {
        return Err(anyhow!("tmux session '{}' not found", session_name));
    }

    // Get the session's pane PID (this will be the shell)
    let pane_pid_output =
        Tmux::with_command(ListPanes::new().target(session_name).format("#{pane_pid}")).output()?;

    if !pane_pid_output.success() {
        return Err(anyhow!(
            "Failed to get tmux pane PID for session '{}'",
            session_name
        ));
    }

    let shell_pid = String::from_utf8(pane_pid_output.stdout().to_vec())?
        .trim()
        .parse::<u32>()
        .map_err(|e| anyhow!("Failed to parse shell PID: {}", e))?;

    // Use pgrep to find gflowd processes
    let output = Command::new("pgrep")
        .args(["-f", "^gflowd -vvv"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("gflowd daemon process not found (tried pgrep)"));
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

    // For each candidate PID, check if it's a child of the shell PID
    for pid in &pids {
        if is_process_descendant_of(*pid, shell_pid) {
            tracing::debug!(
                "Found gflowd PID {} as descendant of tmux session '{}' (shell PID {})",
                pid,
                session_name,
                shell_pid
            );
            return Ok(*pid);
        }
    }

    Err(anyhow!(
        "Could not find gflowd process as descendant of tmux session '{}'",
        session_name
    ))
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
            // Parent PID is the 4th field in /proc/pid/stat
            if let Some(ppid_str) = stat.split_whitespace().nth(3) {
                if let Ok(ppid) = ppid_str.parse::<u32>() {
                    if ppid <= 1 {
                        break; // Reached init
                    }
                    current_pid = ppid;
                    continue;
                }
            }
        }
        break;
    }

    false
}

fn is_process_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}
