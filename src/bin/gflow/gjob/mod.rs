mod cli;
mod commands;
mod utils;

use anyhow::Result;
use clap::Parser;
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GJob::parse_from(argv);

    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .init();

    commands::handle_commands(&args.config, args.command).await?;
    Ok(())
}
