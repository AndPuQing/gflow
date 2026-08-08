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

    // Direct (flock/pidfile) mode first.
    if let Some(pid) = super::lifecycle::direct_daemon_pid() {
        // Re-verify identity immediately before signalling so a PID that was
        // recycled to an unrelated process since the liveness probe is never
        // signalled (flock auto-released on crash; identity check on reuse).
        if !super::lifecycle::verify_before_signal(pid) {
            super::lifecycle::remove_daemon_lock();
            super::lifecycle::remove_daemon_pidfile();
            println!("gflowd is not running (stale lock/pidfile cleaned up).");
            return Ok(());
        }
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
        // Wait for graceful shutdown (the daemon kills managed jobs and
        // saves state), then escalate to SIGKILL if needed.
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
        super::lifecycle::remove_daemon_lock();
        super::lifecycle::remove_daemon_pidfile();
        println!("gflowd stopped.");
        return Ok(());
    }

    if let Err(e) =
        Tmux::with_command(KillSession::new().target_session(super::TMUX_SESSION_NAME)).output()
    {
        eprintln!("Failed to stop gflowd: {e}");
    } else {
        println!("gflowd stopped.");
    }
    Ok(())
}
