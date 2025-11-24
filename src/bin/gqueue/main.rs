mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use gflow::config::load_config;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GQueue::parse();

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::GQueue::command();
                generate(shell, &mut cmd, "gqueue", &mut std::io::stdout());
                return Ok(());
            }
        }
    }

    let config = load_config(args.config.as_ref())?;
    commands::handle_commands(&config, &args.list_args).await?;

    Ok(())
}
