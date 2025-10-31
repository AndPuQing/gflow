use crate::cli::Commands;

pub mod attach;
pub mod log;

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
    }

    Ok(())
}
