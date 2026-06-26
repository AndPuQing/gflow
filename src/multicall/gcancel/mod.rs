mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GCancel::parse_from(argv);

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                crate::multicall::completion::handle_completion(
                    shell,
                    cli::GCancel::command(),
                    "gcancel",
                )?;
                return Ok(());
            }
        }
    }

    let command = args.cancel_args.get_command()?;
    commands::handle_commands(&args.config, command).await?;

    Ok(())
}
