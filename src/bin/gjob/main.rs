mod cli;
mod commands;
mod utils;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GJob::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .init();

    commands::handle_commands(&args.config, args.command).await?;
    Ok(())
}
