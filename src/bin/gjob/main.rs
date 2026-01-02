mod cli;
mod commands;
mod utils;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GJob::parse();

    // Initialize tracing based on verbosity
    let filter = match args.verbose {
        0 => "gflow=info,gjob=info",
        1 => "gflow=debug,gjob=debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .init();

    commands::handle_commands(&args.config, args.command).await?;
    Ok(())
}
