use super::cli::Commands;
use anyhow::{anyhow, Context, Result};
use clap::CommandFactory;
use clap_verbosity_flag::{Verbosity, VerbosityFilter};

pub mod down;
pub mod init;
pub mod reload;
pub mod status;
pub mod up;

pub static TMUX_SESSION_NAME: &str = "gflow_server";

#[derive(Debug, Clone)]
pub struct DaemonStartOptions<'a> {
    pub gpus: Option<&'a str>,
    pub gpu_allocation_strategy: Option<&'a str>,
    pub gpu_poll_interval_secs: Option<u64>,
    pub verbosity: Verbosity,
}

impl<'a> DaemonStartOptions<'a> {
    fn from_overrides(overrides: &'a super::cli::DaemonOverrideArgs, verbosity: Verbosity) -> Self {
        Self {
            gpus: overrides.gpus.as_deref(),
            gpu_allocation_strategy: overrides.gpu_allocation_strategy.as_deref(),
            gpu_poll_interval_secs: overrides.gpu_poll_interval_secs,
            verbosity,
        }
    }
}

pub fn validate_daemon_startup_config(
    config_path: &Option<std::path::PathBuf>,
    options: &DaemonStartOptions<'_>,
) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref())?;
    let gpu_poll_interval_secs = options
        .gpu_poll_interval_secs
        .unwrap_or(config.daemon.gpu_poll_interval_secs);

    if gpu_poll_interval_secs == 0 {
        return Err(anyhow!(
            "Invalid daemon.gpu_poll_interval_secs '0'. Use a value of at least 1 second."
        ));
    }

    Ok(())
}

/// Build a shell command that always starts daemon from the currently running `gflow` binary.
/// This avoids accidentally picking a stale `gflow`/`gflowd` from PATH.
pub fn daemon_start_command(options: &DaemonStartOptions<'_>) -> Result<String> {
    let gflow_path = std::env::current_exe().context("failed to resolve current gflow binary")?;
    let exe = shell_escape::escape(gflow_path.to_string_lossy());

    let mut command = format!("{exe} __multicall gflowd");
    if options.verbosity.is_present() {
        if let Some(flag) = daemon_verbosity_flag(options.verbosity) {
            command.push(' ');
            command.push_str(flag);
        }
    } else {
        // Keep existing behavior for plain `gflowd up`: start daemon with debug logs.
        command.push_str(" -vvv");
    }
    if let Some(gpu_spec) = options.gpus {
        let escaped = shell_escape::escape(gpu_spec.into());
        command.push_str(&format!(" --gpus-internal {escaped}"));
    }
    if let Some(strategy) = options.gpu_allocation_strategy {
        strategy
            .parse::<gflow::core::gpu_allocation::GpuAllocationStrategy>()
            .map_err(|_| {
                anyhow!(
                    "Invalid GPU allocation strategy '{}'. Use 'sequential' or 'random'.",
                    strategy
                )
            })?;
        let escaped = shell_escape::escape(strategy.into());
        command.push_str(&format!(" --gpu-allocation-strategy-internal {escaped}"));
    }
    if let Some(gpu_poll_interval_secs) = options.gpu_poll_interval_secs {
        if gpu_poll_interval_secs == 0 {
            return Err(anyhow!(
                "Invalid GPU poll interval '{}'. Use a value of at least 1 second.",
                gpu_poll_interval_secs
            ));
        }
        command.push_str(&format!(
            " --gpu-poll-interval-secs-internal {}",
            gpu_poll_interval_secs
        ));
    }

    Ok(command)
}

fn daemon_verbosity_flag(verbosity: Verbosity) -> Option<&'static str> {
    match verbosity.filter() {
        VerbosityFilter::Off => Some("-q"),
        VerbosityFilter::Error => None,
        VerbosityFilter::Warn => Some("-v"),
        VerbosityFilter::Info => Some("-vv"),
        VerbosityFilter::Debug => Some("-vvv"),
        VerbosityFilter::Trace => Some("-vvvv"),
    }
}

pub async fn handle_commands(
    config_path: &Option<std::path::PathBuf>,
    verbosity: Verbosity,
    command: Commands,
) -> Result<()> {
    match command {
        Commands::Init {
            yes,
            force,
            advanced,
            host,
            port,
            timezone,
            daemon_overrides,
        } => {
            init::handle_init(
                config_path,
                init::InitArgs {
                    yes,
                    force,
                    advanced,
                    gpus: daemon_overrides.gpus,
                    host,
                    port,
                    timezone,
                    gpu_allocation_strategy: daemon_overrides.gpu_allocation_strategy,
                    gpu_poll_interval_secs: daemon_overrides.gpu_poll_interval_secs,
                },
            )
            .await?;
        }
        Commands::Up(daemon_overrides) => {
            up::handle_up(config_path, daemon_overrides, verbosity).await?;
        }
        Commands::Down => {
            down::handle_down().await?;
        }
        Commands::Restart(daemon_overrides) => {
            down::handle_down().await?;
            up::handle_up(config_path, daemon_overrides, verbosity).await?;
        }
        Commands::Reload(daemon_overrides) => {
            reload::handle_reload(config_path, daemon_overrides, verbosity).await?;
        }
        Commands::Status => {
            status::handle_status(config_path).await?;
        }
        Commands::Completion { shell } => {
            let mut cmd = super::cli::GFlowd::command();
            let _ = crate::multicall::completion::generate_to_stdout(shell, &mut cmd, "gflowd");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_start_command_keeps_existing_default_verbosity() {
        let command = daemon_start_command(&DaemonStartOptions {
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: None,
            verbosity: Verbosity::new(0, 0),
        })
        .unwrap();
        assert!(command.contains("__multicall gflowd -vvv"));
    }

    #[test]
    fn daemon_start_command_passes_explicit_verbosity_to_daemon() {
        let warn_command = daemon_start_command(&DaemonStartOptions {
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: None,
            verbosity: Verbosity::new(1, 0),
        })
        .unwrap();
        assert!(warn_command.contains("__multicall gflowd -v"));
        assert!(!warn_command.contains("__multicall gflowd -vvv"));

        let silent_command = daemon_start_command(&DaemonStartOptions {
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: None,
            verbosity: Verbosity::new(0, 1),
        })
        .unwrap();
        assert!(silent_command.contains("__multicall gflowd -q"));

        let trace_command = daemon_start_command(&DaemonStartOptions {
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: None,
            verbosity: Verbosity::new(9, 0),
        })
        .unwrap();
        assert!(trace_command.contains("__multicall gflowd -vvvv"));
    }

    #[test]
    fn daemon_start_command_passes_gpu_poll_interval_override() {
        let command = daemon_start_command(&DaemonStartOptions {
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: Some(3),
            verbosity: Verbosity::new(0, 0),
        })
        .unwrap();
        assert!(command.contains("--gpu-poll-interval-secs-internal 3"));
    }

    #[test]
    fn daemon_start_command_rejects_zero_gpu_poll_interval() {
        let error = daemon_start_command(&DaemonStartOptions {
            gpus: None,
            gpu_allocation_strategy: None,
            gpu_poll_interval_secs: Some(0),
            verbosity: Verbosity::new(0, 0),
        })
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("Use a value of at least 1 second"));
    }

    #[test]
    fn validate_daemon_startup_config_rejects_zero_poll_interval_from_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("gflow.toml");
        std::fs::write(
            &path,
            r#"
[daemon]
gpu_poll_interval_secs = 0
"#,
        )
        .unwrap();

        let error = validate_daemon_startup_config(
            &Some(path),
            &DaemonStartOptions {
                gpus: None,
                gpu_allocation_strategy: None,
                gpu_poll_interval_secs: None,
                verbosity: Verbosity::new(0, 0),
            },
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("Use a value of at least 1 second"));
    }
}
