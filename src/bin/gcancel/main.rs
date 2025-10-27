mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;
use gflow::config::load_config;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GCancel::parse();
    let config = load_config(args.config.as_ref())?;

    let command = args.get_command()?;
    commands::handle_commands(&config, command).await?;

    Ok(())
}
