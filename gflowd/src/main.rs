use clap::Parser;
use config::load_config;
mod cli;
mod config;
mod job;
mod scheduler;
mod server;

#[tokio::main]
async fn main() {
    let gflowd = cli::GFlowd::parse();
    env_logger::Builder::new()
        .filter_level(gflowd.verbose.log_level_filter())
        .init();
    let config = load_config(gflowd);
    if let Err(e) = config {
        log::error!("Failed to load config: {}", e);
        std::process::exit(1);
    }
    server::run(config.unwrap()).await;
}
