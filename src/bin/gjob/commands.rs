use crate::cli::Commands;

pub mod attach;
pub mod hold;
pub mod log;
pub mod release;

pub async fn handle_commands(
    config_path: &Option<std::path::PathBuf>,
    command: Commands,
) -> anyhow::Result<()> {
    match command {
        Commands::Attach { job } => {
            attach::handle_attach(config_path, job).await?;
        }
        Commands::Log { job } => {
            log::handle_log(config_path, job).await?;
        }
        Commands::Hold { job } => {
            hold::handle_hold(config_path, job).await?;
        }
        Commands::Release { job } => {
            release::handle_release(config_path, job).await?;
        }
    }

    Ok(())
}
