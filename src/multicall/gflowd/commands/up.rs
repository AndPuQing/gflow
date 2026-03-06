use anyhow::{bail, Result};
use clap_verbosity_flag::Verbosity;
use gflow::tmux::{is_session_exist, TmuxSession};

pub async fn handle_up(
    config_path: &Option<std::path::PathBuf>,
    gpus: Option<String>,
    gpu_allocation_strategy: Option<String>,
    verbosity: Verbosity,
) -> Result<()> {
    match existing_daemon_state(config_path).await? {
        ExistingDaemonState::NotPresent => {}
        ExistingDaemonState::Healthy => {
            println!(
                "gflowd is already running in tmux session '{}'.",
                super::TMUX_SESSION_NAME
            );
            println!("Use `gflowd reload` to hot-reload or `gflowd restart` to restart it.");
            return Ok(());
        }
        ExistingDaemonState::Unhealthy(status_code) => {
            bail!(
                "tmux session '{}' already exists, but gflowd health check returned HTTP {}. \
                 Refusing to send another start command. Use `gflowd restart` or `gflowd down` first.",
                super::TMUX_SESSION_NAME,
                status_code
            );
        }
        ExistingDaemonState::Unreachable(error) => {
            bail!(
                "tmux session '{}' already exists, but gflowd is not reachable: {}. \
                 Refusing to send another start command. Use `gflowd down` to clean up the stale session, \
                 or `gflowd restart` to replace it.",
                super::TMUX_SESSION_NAME,
                error
            );
        }
    }

    let session = TmuxSession::create(super::TMUX_SESSION_NAME.to_string())?;
    let command = super::daemon_start_command(
        gpus.as_deref(),
        gpu_allocation_strategy.as_deref(),
        verbosity,
    )?;

    session.try_send_command(&command)?;

    println!("gflowd started.");
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
    if !is_session_exist(super::TMUX_SESSION_NAME) {
        return Ok(ExistingDaemonState::NotPresent);
    }

    let config = gflow::config::load_config(config_path.as_ref()).unwrap_or_default();
    let client = gflow::Client::build(&config)?;

    Ok(match client.get_health().await {
        Ok(status) if status.is_success() => ExistingDaemonState::Healthy,
        Ok(status) => ExistingDaemonState::Unhealthy(status.as_u16()),
        Err(error) => ExistingDaemonState::Unreachable(error.to_string()),
    })
}
