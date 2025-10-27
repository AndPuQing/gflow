mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;
use gflow::config::load_config;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GQueue::parse();
    let config = load_config(args.config.as_ref())?;

    commands::handle_commands(&config, &args).await?;

    Ok(())
}
