mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use gflow::config::load_config;
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GCancel::parse_from(argv);

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::GCancel::command();
                let _ =
                    crate::multicall::completion::generate_to_stdout(shell, &mut cmd, "gcancel");
                return Ok(());
            }
        }
    }

    let config = load_config(args.config.as_ref())?;

    let command = args.cancel_args.get_command()?;
    commands::handle_commands(&config, command).await?;

    Ok(())
}
