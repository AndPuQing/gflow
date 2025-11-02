mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GInfoCli::parse();
    commands::info::handle_info(&args.config).await?;
    Ok(())
}
