mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GInfoCli::parse();

    // Initialize tracing based on verbosity
    let filter = match args.verbose {
        0 => "gflow=info,ginfo=info",
        1 => "gflow=debug,ginfo=debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
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
