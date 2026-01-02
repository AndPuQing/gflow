use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    let gctl = cli::GCtl::parse();

    // Initialize tracing for client binary
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = gflow::config::load_config(gctl.config.as_ref())?;
    let client = gflow::client::Client::build(&config)?;

    commands::handle_commands(&client, gctl.command).await
}
