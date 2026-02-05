use anyhow::Result;
use clap::Parser;
use std::ffi::OsString;
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod reserve_timeline;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let gctl = cli::GCtl::parse_from(argv);

    // Initialize tracing for client binary
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = gflow::config::load_config(gctl.config.as_ref())?;
    let client = gflow::Client::build(&config)?;

    commands::handle_commands(&client, &config, gctl.command).await
}
