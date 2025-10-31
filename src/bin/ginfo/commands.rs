use crate::cli::Commands;

pub mod info;

pub async fn handle_commands(
    config_path: &Option<std::path::PathBuf>,
    command: Commands,
) -> anyhow::Result<()> {
    match command {
        Commands::Info => {
            info::handle_info(config_path).await?;
        }
    }

    Ok(())
}
