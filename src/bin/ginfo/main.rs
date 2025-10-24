use anyhow::Result;
use clap::Parser;
use gflow::config::load_config;

mod cli;
mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GInfo::parse();
    let config = load_config(args.config.as_ref())?;

    commands::handle_command(&config, args.command).await
}
