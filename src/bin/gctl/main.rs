use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod reserve_timeline;

#[tokio::main]
async fn main() -> Result<()> {
    let gctl = cli::GCtl::parse();

    // Initialize tracing for client binary
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let client = gflow::create_client(&gctl.config)?;

    commands::handle_commands(&client, gctl.command).await
}
