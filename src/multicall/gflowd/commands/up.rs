use anyhow::{bail, Context, Result};
use clap_verbosity_flag::Verbosity;
use gflow::tmux::{is_session_exist, TmuxSession};
use std::time::Duration;

pub async fn handle_up(
    config_path: &Option<std::path::PathBuf>,
    daemon_overrides: super::super::cli::DaemonOverrideArgs,
    verbosity: Verbosity,
) -> Result<()> {
    // systemd user service takes priority when installed and active.
    if super::systemd::unit_installed()? && super::systemd::is_active()? {
        println!("gflowd is already running (systemd user service).");
        println!("Use `gflowd restart` to restart it.");
        return Ok(());
    }
    if super::systemd::unit_installed()? {
        println!("Starting via systemd user service...");
        super::systemd::start()?;
        println!("gflowd started (systemd user service).");
        return Ok(());
    }

    match existing_daemon_state(config_path).await? {
        ExistingDaemonState::NotPresent => {}
        ExistingDaemonState::Healthy => {
            println!("gflowd is already running.");
            println!("Use `gflowd reload` to hot-reload or `gflowd restart` to restart it.");
            return Ok(());
        }
        ExistingDaemonState::Unhealthy(status_code) => {
            bail!(
                "an existing gflowd instance is unhealthy (HTTP {}). \
                 Refusing to start another instance. Use `gflowd restart` or `gflowd down` first.",
                status_code
            );
        }
        ExistingDaemonState::Unreachable(error) => {
            bail!(
                "an existing gflowd instance is not reachable: {}. \
                 Refusing to start another instance. Use `gflowd down` to clean up the stale \
                 instance, or `gflowd restart` to replace it.",
                error
            );
        }
    }

    let start_options = super::DaemonStartOptions::from_overrides(&daemon_overrides, verbosity);
    super::validate_daemon_startup_config(config_path, &start_options)?;

    if gflow::tmux::tmux_available() {
        let command = super::daemon_start_command(&start_options)?;
        let session = TmuxSession::create(super::TMUX_SESSION_NAME.to_string())?;

        session.try_send_command(&command)?;

        println!(
            "gflowd started in tmux session '{}'.",
            super::TMUX_SESSION_NAME
        );
    } else {
        start_daemon_direct(&start_options)?;
    }

    Ok(())
}

/// Host the daemon as a detached process (no tmux). The daemon itself takes
/// an exclusive flock on `gflowd.lock` and writes its identity there, so
/// `gflowd down` / `gflowd status` can manage it safely (no stale pidfile,
/// no PID-reuse mis-kills).
fn start_daemon_direct(options: &super::DaemonStartOptions<'_>) -> Result<()> {
    let exe = std::env::current_exe().context("failed to resolve current gflow binary")?;
    let mut args = super::daemon_start_args(options)?;
    // Internal flag: tells the daemon it is being hosted directly and must
    // hold the daemon flock for its lifetime.
    args.push("--direct-internal".to_string());

    let data_dir = gflow::paths::get_data_dir()?;
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let out_path = log_dir.join("daemon.out.log");
    let out_file = std::fs::File::create(&out_path)
        .with_context(|| format!("Failed to create daemon log {}", out_path.display()))?;
    let err_file = out_file
        .try_clone()
        .context("Failed to clone daemon log handle")?;

    let mut command = std::process::Command::new(&exe);
    command
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(out_file))
        .stderr(std::process::Stdio::from(err_file));

    // Detach into its own session so the daemon survives the launching shell.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid() is async-signal-safe and runs in the freshly
        // forked single-threaded child before exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let child = command
        .spawn()
        .context("Failed to spawn gflowd daemon process")?;
    let pid = child.id();

    // Wait briefly for the child to acquire the daemon lock and write its
    // identity. This confirms a healthy start and detects a duplicate (the
    // child bails if another direct daemon already holds the lock).
    let mut acquired = false;
    for _ in 0..30 {
        if let Some(identity) = super::lifecycle::read_daemon_identity() {
            if identity.pid == pid && super::lifecycle::process_identity_matches(&identity) {
                acquired = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !acquired {
        bail!(
            "gflowd daemon (PID {}) did not acquire its lock within 3s; check {}",
            pid,
            out_path.display()
        );
    }

    println!("gflowd started (direct mode, PID {}).", pid);
    println!("Logs: {}", out_path.display());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExistingDaemonState {
    NotPresent,
    Healthy,
    Unhealthy(u16),
    Unreachable(String),
}

async fn existing_daemon_state(
    config_path: &Option<std::path::PathBuf>,
) -> Result<ExistingDaemonState> {
    // Direct (flock/pidfile) mode first.
    if super::lifecycle::direct_daemon_pid().is_some() {
        let client = gflow::create_client_or_default(config_path)?;
        return Ok(match client.get_health().await {
            Ok(status) if status.is_success() => ExistingDaemonState::Healthy,
            Ok(status) => ExistingDaemonState::Unhealthy(status.as_u16()),
            Err(error) => ExistingDaemonState::Unreachable(error.to_string()),
        });
    }

    if !is_session_exist(super::TMUX_SESSION_NAME) {
        return Ok(ExistingDaemonState::NotPresent);
    }

    let client = gflow::create_client_or_default(config_path)?;

    Ok(match client.get_health().await {
        Ok(status) if status.is_success() => ExistingDaemonState::Healthy,
        Ok(status) => ExistingDaemonState::Unhealthy(status.as_u16()),
        Err(error) => ExistingDaemonState::Unreachable(error.to_string()),
    })
}
