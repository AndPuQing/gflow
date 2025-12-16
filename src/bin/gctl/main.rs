use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    let gctl = cli::GCtl::parse();
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = gflow::config::load_config(gctl.config.as_ref())?;
    let client = gflow::client::Client::build(&config)?;

    commands::handle_commands(&client, gctl.command).await
}
