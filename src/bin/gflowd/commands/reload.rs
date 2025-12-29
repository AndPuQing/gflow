use anyhow::{anyhow, Result};
use gflow::tmux::TmuxSession;
use std::process::Command;
use std::time::Duration;

pub async fn handle_reload(gpus: Option<String>) -> Result<()> {
    // 1. Check if daemon is running
    let pid = get_daemon_pid().await?;
    log::info!("Found running daemon at PID {}", pid);

    // 2. Start new daemon instance in temporary tmux session
    log::info!("Starting new daemon instance...");
    let new_session_name = format!("gflow_server_new_{}", std::process::id());
    let session = TmuxSession::new(new_session_name.clone());

    let mut command = String::from("gflowd -vvv");
    if let Some(gpu_spec) = gpus {
        command.push_str(&format!(" --gpus-internal '{}'", gpu_spec));
    }

    session.send_command(&command);

    // 3. Wait for new instance to bind socket (SO_REUSEPORT allows this)
    log::info!("Waiting for new instance to initialize...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 4. Verify new instance is healthy
    if !is_daemon_healthy().await {
        log::error!("New daemon instance failed health check, aborting reload");
        // Kill the new session
        gflow::tmux::kill_session(&new_session_name).ok();
        return Err(anyhow!(
            "New daemon instance failed health check. Old daemon is still running."
        ));
    }
    log::info!("New daemon instance is healthy");

    // 5. Signal old process to shutdown (SIGUSR2)
    log::info!("Signaling old daemon (PID {}) to shutdown", pid);
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGUSR2);
    }

    // 6. Wait for old process to exit
    let mut exited = false;
    for i in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !is_process_running(pid) {
            log::info!("Old daemon has exited");
            exited = true;
            break;
        }
        if i == 29 {
            log::warn!(
                "Old daemon did not exit within 3 seconds. \
                 New daemon is running, but old process may need manual cleanup."
            );
        }
    }

    // 7. Rename new tmux session to standard name
    // First, ensure old session is gone
    gflow::tmux::kill_session(super::TMUX_SESSION_NAME).ok();

    let rename_result = Command::new("tmux")
        .args([
            "rename-session",
            "-t",
            &new_session_name,
            super::TMUX_SESSION_NAME,
        ])
        .output();

    match rename_result {
        Ok(output) if output.status.success() => {
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
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "Failed to rename new session: {}. \
                 New daemon is running as session '{}', you may need to rename it manually.",
                stderr,
                new_session_name
            ))
        }
        Err(e) => Err(anyhow!(
            "Failed to execute tmux rename: {}. \
             New daemon is running as session '{}', you may need to rename it manually.",
            e,
            new_session_name
        )),
    }
}

async fn get_daemon_pid() -> Result<u32> {
    // Get PID from tmux session
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            super::TMUX_SESSION_NAME,
            "-F",
            "#{pane_pid}",
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "gflowd is not running (tmux session '{}' not found)",
            super::TMUX_SESSION_NAME
        ));
    }

    let pid_str = String::from_utf8(output.stdout)?;
    let pid = pid_str
        .trim()
        .parse::<u32>()
        .map_err(|e| anyhow!("Failed to parse PID from tmux: {}", e))?;
    Ok(pid)
}

fn is_process_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

async fn is_daemon_healthy() -> bool {
    // Try to connect to health endpoint
    match reqwest::get("http://localhost:59000/health").await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}
