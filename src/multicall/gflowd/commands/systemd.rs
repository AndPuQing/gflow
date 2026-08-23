use super::super::cli::ServiceAction;
use super::DaemonStartOptions;
use anyhow::{anyhow, bail, Context, Result};
use clap_verbosity_flag::Verbosity;
use std::path::PathBuf;

/// Name of the systemd user unit.
pub const SYSTEMD_UNIT: &str = "gflowd.service";

/// Path of the installed user unit file (`~/.config/systemd/user/gflowd.service`).
pub fn systemd_unit_path() -> Result<PathBuf> {
    let config_dir = gflow::paths::get_config_dir()?;
    // Walk one level up from `~/.config/gflow` to `~/.config`, then into
    // systemd's user-unit directory.
    let base = config_dir
        .parent()
        .ok_or_else(|| anyhow!("Failed to resolve config directory"))?;
    Ok(base.join("systemd").join("user").join(SYSTEMD_UNIT))
}

/// Whether a systemd user manager is reachable from this environment.
///
/// Requires the `systemctl` binary and a live user systemd socket
/// (`$XDG_RUNTIME_DIR/systemd/private`). This is the heuristic used to decide
/// whether `gflowd start`/`stop`/`status` can delegate to systemd.
pub fn systemd_user_available() -> bool {
    let has_systemctl = std::process::Command::new("systemctl")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !has_systemctl {
        return false;
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    match runtime {
        Some(runtime) => runtime.join("systemd").join("private").exists(),
        None => false,
    }
}

/// Whether the `gflowd.service` user unit has been installed.
pub fn unit_installed() -> Result<bool> {
    Ok(systemd_unit_path()?.exists())
}

/// Whether the installed unit is currently active.
pub fn is_active() -> Result<bool> {
    let output = std::process::Command::new("systemctl")
        .arg("--user")
        .args(["is-active", SYSTEMD_UNIT])
        .output()
        .context("failed to run systemctl --user is-active")?;
    Ok(output.status.success())
}

fn systemctl(args: &[&str]) -> Result<std::process::Output> {
    let output = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .with_context(|| format!("failed to run systemctl --user {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "systemctl --user {} failed: {}",
            args.join(" "),
            if stderr.is_empty() {
                "non-zero exit status".to_string()
            } else {
                stderr
            }
        );
    }
    Ok(output)
}

pub fn start() -> Result<()> {
    systemctl(&["start", SYSTEMD_UNIT]).map(|_| ())
}

pub fn stop() -> Result<()> {
    systemctl(&["stop", SYSTEMD_UNIT]).map(|_| ())
}

/// Build the `ExecStart` line for the unit, reusing the daemon start args
/// (including `-c <config>`) and always pointing at the currently running
/// `gflow` binary.
fn unit_exec_start(options: &DaemonStartOptions<'_>) -> Result<String> {
    let exe = std::env::current_exe().context("failed to resolve current gflow binary")?;
    let mut parts: Vec<String> = vec![shell_escape::escape(exe.to_string_lossy()).into_owned()];
    for arg in super::daemon_start_args(options)? {
        parts.push(shell_escape::escape(arg.into()).into_owned());
    }
    Ok(parts.join(" "))
}

fn unit_file_content(exec_start: &str) -> String {
    format!(
        r#"[Unit]
Description=GFlow daemon
After=network.target

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
"#
    )
}

pub async fn handle_service(
    config_path: &Option<std::path::PathBuf>,
    verbosity: Verbosity,
    action: ServiceAction,
) -> Result<()> {
    match action {
        ServiceAction::Install(args) => {
            if !systemd_user_available() {
                bail!(
                    "systemd user services are not available here (no user systemd manager). \
                     gflowd will host the daemon via tmux or as a direct process instead; \
                     run `gflowd start` to start it."
                );
            }
            let start_options = super::DaemonStartOptions::from_overrides(
                config_path,
                &args.daemon_overrides,
                verbosity,
            );
            super::validate_daemon_startup_config(config_path, &start_options)?;

            let exec_start = unit_exec_start(&start_options)?;
            let path = systemd_unit_path()?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create {}", parent.display()))?;
            }
            std::fs::write(&path, unit_file_content(&exec_start))
                .with_context(|| format!("Failed to write systemd unit {}", path.display()))?;

            systemctl(&["daemon-reload"])?;
            systemctl(&["enable", "--now", SYSTEMD_UNIT])?;
            println!("gflowd systemd user service installed and started.");
            println!("Unit: {}", path.display());
            println!("gflowd will auto-start on login and restart on crash.");
        }
        ServiceAction::Uninstall => {
            if !systemd_user_available() {
                bail!("systemd user services are not available on this system.");
            }
            let path = systemd_unit_path()?;
            // Best-effort: stop and disable even if the unit is gone.
            let _ = systemctl(&["stop", SYSTEMD_UNIT]);
            let _ = systemctl(&["disable", SYSTEMD_UNIT]);
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove systemd unit {}", path.display()))?;
            }
            let _ = systemctl(&["daemon-reload"]);
            println!("gflowd systemd user service removed.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_file_content_embeds_exec_start() {
        let content = unit_file_content("/path/to/gflow __multicall gflowd -vvv");
        assert!(content.contains("Description=GFlow daemon"));
        assert!(content.contains("Type=simple"));
        assert!(content.contains("ExecStart=/path/to/gflow __multicall gflowd -vvv"));
        assert!(content.contains("Restart=on-failure"));
        assert!(content.contains("RestartSec=2"));
        assert!(content.contains("WantedBy=default.target"));
    }

    #[test]
    fn unit_exec_start_reuses_daemon_start_command() {
        // Ensure the unit's ExecStart matches the tmux/direct daemon command
        // token-for-token, so the hosting layer is transparent to the user.
        let options = DaemonStartOptions {
            config_path: None,
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: None,
            verbosity: Verbosity::new(0, 0),
        };
        let exec_start = super::unit_exec_start(&options).unwrap();
        let daemon_cmd = super::super::daemon_start_command(&options).unwrap();
        assert_eq!(exec_start, daemon_cmd);
        assert!(exec_start.contains("__multicall gflowd -vvv"));
    }

    #[test]
    fn unit_exec_start_passes_config_path() {
        let options = DaemonStartOptions {
            config_path: Some(std::path::Path::new("/srv/gflow/custom.toml")),
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: None,
            verbosity: Verbosity::new(0, 0),
        };
        let exec_start = super::unit_exec_start(&options).unwrap();
        assert!(exec_start.contains("__multicall gflowd -c /srv/gflow/custom.toml"));
    }
}
