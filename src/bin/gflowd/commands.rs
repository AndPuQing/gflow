use crate::cli::Commands;
use clap::CommandFactory;
use clap_complete::generate;

pub mod down;
pub mod status;
pub mod up;

pub static TMUX_SESSION_NAME: &str = "gflow_server";

pub async fn handle_commands(
    config_path: &Option<std::path::PathBuf>,
    command: Commands,
) -> anyhow::Result<()> {
    match command {
        Commands::Up => {
            up::handle_up().await?;
        }
        Commands::Down => {
            down::handle_down().await?;
        }
        Commands::Restart => {
            down::handle_down().await?;
            up::handle_up().await?;
        }
        Commands::Status => {
            status::handle_status(config_path).await?;
        }
        Commands::Completion { shell } => {
            let mut cmd = crate::cli::GFlowd::command();
            generate(shell, &mut cmd, "gflowd", &mut std::io::stdout());
        }
    }

    Ok(())
}
