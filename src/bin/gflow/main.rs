use clap::{CommandFactory, Parser};
use cli::GFlow;
use commands::handle_commands;
mod cli;
mod client;
mod commands;
mod help;
mod tui;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let gflow = GFlow::parse();

    // Initialize logger
    env_logger::Builder::new()
        .filter_level(gflow.verbose.log_level_filter())
        .init();

    log::debug!("Starting gflow with args: {:?}", gflow);

    // Handle commands if present
    if let Some(commands) = gflow.commands {
        let output = handle_commands(commands).await;
        if let Err(e) = output {
            log::error!("Error: {}", e);
            std::process::exit(1);
        }
    } else {
        // Show help when no command is provided
        let _ = GFlow::command().print_help();
    }
}
