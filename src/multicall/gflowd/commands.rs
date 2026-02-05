use super::cli::Commands;
use clap::CommandFactory;

pub mod down;
pub mod reload;
pub mod status;
pub mod up;

pub static TMUX_SESSION_NAME: &str = "gflow_server";

pub async fn handle_commands(
    config_path: &Option<std::path::PathBuf>,
    command: Commands,
) -> anyhow::Result<()> {
    match command {
        Commands::Up { gpus } => {
            up::handle_up(gpus).await?;
        }
        Commands::Down => {
            down::handle_down().await?;
        }
        Commands::Restart { gpus } => {
            down::handle_down().await?;
            up::handle_up(gpus).await?;
        }
        Commands::Reload { gpus } => {
            reload::handle_reload(config_path, gpus).await?;
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
