mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use gflow::config::load_config;
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GQueue::parse_from(argv);

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::GQueue::command();
                let _ = crate::multicall::completion::generate_to_stdout(shell, &mut cmd, "gqueue");
                return Ok(());
            }
        }
    }

    let config = load_config(args.config.as_ref())?;
    commands::handle_commands(&config, &args.list_args).await?;

    Ok(())
}
