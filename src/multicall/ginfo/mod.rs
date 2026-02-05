mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GInfoCli::parse_from(argv);

    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .init();

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::GInfoCli::command();
                let _ = crate::multicall::completion::generate_to_stdout(shell, &mut cmd, "ginfo");
                return Ok(());
            }
        }
    }

    commands::info::handle_info(&args.config).await?;
    Ok(())
}
