use anyhow::Result;
use clap::Parser;
use commands::handle_commands;
use gflow::config::load_config;
mod cli;
mod commands;
mod history;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GBatch::parse();
    let config = load_config(args.config.as_ref())?;

    if let Some(commands) = args.commands {
        handle_commands(&config, commands).await
    } else {
        commands::add::handle_add(&config, args.add_args).await
    }
}
