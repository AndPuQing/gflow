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

/// Path of the pidfile used when the daemon is hosted without tmux.
pub fn daemon_pidfile_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(gflow::paths::get_runtime_dir()?.join("gflowd.pid"))
}

pub fn read_daemon_pidfile() -> Option<u32> {
    let content = std::fs::read_to_string(daemon_pidfile_path().ok()?).ok()?;
    content.trim().parse().ok()
}

pub fn write_daemon_pidfile(pid: u32) -> anyhow::Result<()> {
    let path = daemon_pidfile_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, pid.to_string())?;
    Ok(())
}

pub fn remove_daemon_pidfile() {
    if let Ok(path) = daemon_pidfile_path() {
        let _ = std::fs::remove_file(path);
    }
}

pub fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

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

/// Build argv for the daemon process (used both by the tmux-hosted shell
/// command and by the direct (no-tmux) spawn).
pub fn daemon_start_args(options: &DaemonStartOptions<'_>) -> Result<Vec<String>> {
    let mut args = vec!["__multicall".to_string(), "gflowd".to_string()];

    if options.verbosity.is_present() {
        if let Some(flag) = daemon_verbosity_flag(options.verbosity) {
            args.push(flag.to_string());
        }
    } else {
        // Keep existing behavior for plain `gflowd up`: start daemon with debug logs.
        args.push("-vvv".to_string());
    }
    if let Some(gpu_spec) = options.gpus {
        args.push("--gpus-internal".to_string());
        args.push(gpu_spec.to_string());
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
        args.push("--gpu-allocation-strategy-internal".to_string());
        args.push(strategy.to_string());
    }
    if let Some(gpu_poll_interval_secs) = options.gpu_poll_interval_secs {
        if gpu_poll_interval_secs == 0 {
            return Err(anyhow!(
                "Invalid GPU poll interval '{}'. Use a value of at least 1 second.",
                gpu_poll_interval_secs
            ));
        }
        args.push("--gpu-poll-interval-secs-internal".to_string());
        args.push(gpu_poll_interval_secs.to_string());
    }

    Ok(args)
}

/// Build a shell command that always starts daemon from the currently running `gflow` binary.
/// This avoids accidentally picking a stale `gflow`/`gflowd` from PATH.
pub fn daemon_start_command(options: &DaemonStartOptions<'_>) -> Result<String> {
    let gflow_path = std::env::current_exe().context("failed to resolve current gflow binary")?;
    let exe = shell_escape::escape(gflow_path.to_string_lossy());

    let mut command = format!("{exe}");
    for arg in daemon_start_args(options)? {
        command.push(' ');
        command.push_str(&shell_escape::escape(arg.into()));
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
            crate::multicall::completion::handle_completion(
                shell,
                super::cli::GFlowd::command(),
                "gflowd",
            )?;
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
