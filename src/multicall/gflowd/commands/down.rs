use anyhow::Result;
use std::time::Duration;
use tmux_interface::{KillSession, Tmux};

pub async fn handle_down() -> Result<()> {
    // systemd user service takes priority when installed and active.
    if super::systemd::unit_installed().unwrap_or(false)
        && super::systemd::is_active().unwrap_or(false)
    {
        super::systemd::stop()?;
        println!("gflowd stopped.");
        return Ok(());
    }

    // Whichever daemon holds the instance lock is the one to stop. Its
    // recorded hosting mode says how: signal the PID for a detached direct
    // daemon, or let the tmux session die for a supervised one (so the session
    // does not linger as a dead shell).
    if let Some((pid, mode)) = super::lifecycle::locked_daemon() {
        match mode {
            super::lifecycle::DaemonHostMode::Direct => {
                stop_direct_daemon(pid).await;
            }
            super::lifecycle::DaemonHostMode::Supervised => {
                stop_supervised_daemon(pid).await;
            }
        }
        println!("gflowd stopped.");
        return Ok(());
    }

    // No daemon holds the lock. Still kill the tmux session if one exists, to
    // cover a daemon from an older build that never took the lock.
    if let Err(e) =
        Tmux::with_command(KillSession::new().target_session(super::TMUX_SESSION_NAME)).output()
    {
        eprintln!("Failed to stop gflowd: {e}");
    } else {
        println!("gflowd stopped.");
    }
    Ok(())
}

/// Stop a detached, directly-hosted daemon by signalling its PID.
async fn stop_direct_daemon(pid: u32) {
    // Re-verify identity immediately before signalling so a PID that was
    // recycled to an unrelated process since the liveness probe is never
    // signalled (flock auto-released on crash; identity check on reuse).
    if !super::lifecycle::verify_before_signal(pid) {
        super::lifecycle::clear_daemon_identity();
        return;
    }
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    // Wait for graceful shutdown (the daemon kills managed jobs and saves
    // state), then escalate to SIGKILL if needed.
    let mut exited = false;
    for _ in 0..50 {
        if !super::lifecycle::process_alive(pid) {
            exited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !exited {
        eprintln!(
            "gflowd (PID {}) did not exit within 5s, sending SIGKILL",
            pid
        );
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    super::lifecycle::clear_daemon_identity();
}

/// Stop a supervised daemon by ending its tmux session, falling back to
/// signalling the recorded PID when the session is already gone.
async fn stop_supervised_daemon(pid: u32) {
    let _ =
        Tmux::with_command(KillSession::new().target_session(super::TMUX_SESSION_NAME)).output();

    // Wait for the daemon to actually exit (it saves state on SIGHUP/SIGTERM)
    // before clearing the lock file, so a new daemon cannot start against a
    // still-running predecessor.
    for _ in 0..50 {
        if !super::lifecycle::process_alive(pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    if super::lifecycle::process_alive(pid) {
        eprintln!(
            "gflowd (PID {}) did not exit after its tmux session was killed; sending SIGTERM",
            pid
        );
        if super::lifecycle::verify_before_signal(pid) {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
    }

    super::lifecycle::clear_daemon_identity();
}
