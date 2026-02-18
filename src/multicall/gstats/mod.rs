mod cli;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GStats::parse_from(argv);

    if let Some(command) = args.command {
        match command {
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::GStats::command();
                let _ = crate::multicall::completion::generate_to_stdout(shell, &mut cmd, "gstats");
                return Ok(());
            }
        }
    }

    commands::stats::handle_stats(
        &args.config,
        args.user.as_deref(),
        args.all_users,
        args.since.as_deref(),
        &args.output,
    )
    .await?;

    Ok(())
}
