use clap::Parser;
use cli::GFlow;
use commands::handle_commands;
mod cli;
mod client;
mod commands;
mod help;

#[tokio::main]
async fn main() {
    let gflow = GFlow::parse();
    env_logger::Builder::new()
        .filter_level(gflow.verbose.log_level_filter())
        .init();

    log::debug!("{:?}", gflow);

    if let Some(commands) = gflow.commands {
        handle_commands(commands).await;
    }
}
