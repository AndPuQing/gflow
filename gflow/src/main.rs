use clap::Parser;
use cli::GFlow;
use commands::handle_commands;
mod cli;
mod commands;
mod help;

fn main() {
    let gflow = GFlow::parse();
    env_logger::Builder::new()
        .filter_level(gflow.verbose.log_level_filter())
        .init();

    log::debug!("{:?}", gflow);

    if let Some(commands) = gflow.commands {
        handle_commands(commands);
    }
}
