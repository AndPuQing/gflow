mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GInfoCli::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .init();

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::GInfoCli::command();
                generate(shell, &mut cmd, "ginfo", &mut std::io::stdout());
                return Ok(());
            }
        }
    }

    commands::info::handle_info(&args.config).await?;
    Ok(())
}
