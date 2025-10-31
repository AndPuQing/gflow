mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GJob::parse();
    commands::handle_commands(&args.config, args.command).await?;
    Ok(())
}
