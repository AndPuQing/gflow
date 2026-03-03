use super::cli::Commands;
use anyhow::{anyhow, Context, Result};
use clap::CommandFactory;

pub mod down;
pub mod init;
pub mod reload;
pub mod status;
pub mod up;

pub static TMUX_SESSION_NAME: &str = "gflow_server";

/// Build a shell command that always starts daemon from the currently running `gflow` binary.
/// This avoids accidentally picking a stale `gflow`/`gflowd` from PATH.
pub fn daemon_start_command(
    gpus: Option<&str>,
    gpu_allocation_strategy: Option<&str>,
) -> Result<String> {
    let gflow_path = std::env::current_exe().context("failed to resolve current gflow binary")?;
    let exe = shell_escape::escape(gflow_path.to_string_lossy());

    let mut command = format!("{exe} __multicall gflowd -v");
    if let Some(gpu_spec) = gpus {
        let escaped = shell_escape::escape(gpu_spec.into());
        command.push_str(&format!(" --gpus-internal {escaped}"));
    }
    if let Some(strategy) = gpu_allocation_strategy {
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

    Ok(command)
}

pub async fn handle_commands(
    config_path: &Option<std::path::PathBuf>,
    command: Commands,
) -> Result<()> {
    match command {
        Commands::Init {
            yes,
            force,
            advanced,
            gpus,
            host,
            port,
            timezone,
            gpu_allocation_strategy,
        } => {
            init::handle_init(
                config_path,
                init::InitArgs {
                    yes,
                    force,
                    advanced,
                    gpus,
                    host,
                    port,
                    timezone,
                    gpu_allocation_strategy,
                },
            )
            .await?;
        }
        Commands::Up {
            gpus,
            gpu_allocation_strategy,
        } => {
            up::handle_up(gpus, gpu_allocation_strategy).await?;
        }
        Commands::Down => {
            down::handle_down().await?;
        }
        Commands::Restart {
            gpus,
            gpu_allocation_strategy,
        } => {
            down::handle_down().await?;
            up::handle_up(gpus, gpu_allocation_strategy).await?;
        }
        Commands::Reload {
            gpus,
            gpu_allocation_strategy,
        } => {
            reload::handle_reload(config_path, gpus, gpu_allocation_strategy).await?;
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
