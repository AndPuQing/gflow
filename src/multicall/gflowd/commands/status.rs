use anyhow::Result;
use gflow::client::Client;
use gflow::core::info::DaemonStatus;
use gflow::tmux::is_session_exist;

fn format_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if days > 0 {
        format!("{days}d{hours}h{minutes}m{seconds}s")
    } else if hours > 0 {
        format!("{hours}h{minutes}m{seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn print_daemon_summary(status: &DaemonStatus) {
    println!("Version:   {}", status.version);
    println!("PID:       {}", status.pid);
    println!("Uptime:    {}", format_uptime(status.uptime_secs));
    println!("Executor:  {}", status.executor);
    println!(
        "GPUs:      {} total, {} available",
        status.gpu_total, status.gpu_available
    );
}

pub async fn handle_status(config_path: &Option<std::path::PathBuf>) -> Result<()> {
    // systemd user service takes priority.
    if super::systemd::unit_installed().unwrap_or(false)
        && super::systemd::is_active().unwrap_or(false)
    {
        let client = gflow::create_client_or_default(config_path)?;
        match client.get_health().await {
            Ok(health) if health.is_success() => {
                println!("Status: Running");
                println!(
                    "Hosting: systemd user service ({}).",
                    super::systemd::SYSTEMD_UNIT
                );
                print_daemon_summary(&fetch_summary(&client).await);
            }
            Ok(_) => {
                println!("Status: Unhealthy");
                eprintln!("The gflowd daemon responded to the health check but is not healthy.");
            }
            Err(e) => {
                println!("Status: Not Running");
                eprintln!("Failed to connect to gflowd daemon: {e}");
            }
        }
        return Ok(());
    }

    // Direct (flock/pidfile) mode first.
    if let Some(pid) = super::lifecycle::direct_daemon_pid() {
        let client = gflow::create_client_or_default(config_path)?;

        match client.get_health().await {
            Ok(health) if health.is_success() => {
                println!("Status: Running");
                println!(
                    "The gflowd daemon is running as a direct process (PID {}).",
                    pid
                );
                print_daemon_summary(&fetch_summary(&client).await);
            }
            Ok(_) => {
                println!("Status: Unhealthy");
                eprintln!("The gflowd daemon responded to the health check but is not healthy.");
            }
            Err(e) => {
                println!("Status: Not Running");
                eprintln!("Failed to connect to gflowd daemon: {e}");
            }
        }
        return Ok(());
    }

    let session_exists = is_session_exist(super::TMUX_SESSION_NAME);

    if !session_exists {
        println!("Status: Not running");
        println!("The gflowd daemon is not running.");
        return Ok(());
    }

    let client = gflow::create_client_or_default(config_path)?;

    match client.get_health().await {
        Ok(health) => {
            if health.is_success() {
                println!("Status: Running");
                println!(
                    "The gflowd daemon is running in tmux session '{}'.",
                    super::TMUX_SESSION_NAME
                );
                print_daemon_summary(&fetch_summary(&client).await);
            } else {
                println!("Status: Unhealthy");
                eprintln!("The gflowd daemon responded to the health check but is not healthy.");
            }
        }
        Err(e) => {
            println!("Status: Not Running");
            eprintln!("Failed to connect to gflowd daemon: {e}");
        }
    }
    Ok(())
}

/// Best-effort fetch of the rich daemon summary. Falls back to a placeholder
/// struct if the daemon is an older version without `GET /status`.
async fn fetch_summary(client: &Client) -> DaemonStatus {
    match client.get_daemon_status().await {
        Ok(status) => status,
        Err(_) => DaemonStatus {
            version: gflow::build_info::version()
                .lines()
                .next()
                .unwrap_or_default()
                .to_string(),
            pid: 0,
            uptime_secs: 0,
            executor: String::new(),
            gpu_total: 0,
            gpu_available: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_variants() {
        assert_eq!(format_uptime(0), "0s");
        assert_eq!(format_uptime(59), "59s");
        assert_eq!(format_uptime(90), "1m30s");
        assert_eq!(format_uptime(3600), "1h0m0s");
        assert_eq!(format_uptime(3630), "1h0m30s");
        assert_eq!(format_uptime(3661), "1h1m1s");
        assert_eq!(format_uptime(86_400), "1d0h0m0s");
        assert_eq!(format_uptime(93_383), "1d1h56m23s");
    }
}
