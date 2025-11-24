mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use gflow::config::load_config;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GCancel::parse();

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::GCancel::command();
                generate(shell, &mut cmd, "gcancel", &mut std::io::stdout());
                return Ok(());
            }
        }
    }

    let config = load_config(args.config.as_ref())?;

    let command = args.cancel_args.get_command()?;
    commands::handle_commands(&config, command).await?;

    Ok(())
}
