mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GQueue::parse_from(argv);

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                crate::multicall::completion::handle_completion(
                    shell,
                    cli::GQueue::command(),
                    "gqueue",
                )?;
                return Ok(());
            }
        }
    }

    commands::handle_commands(&args.config, &args.list_args).await?;

    Ok(())
}
