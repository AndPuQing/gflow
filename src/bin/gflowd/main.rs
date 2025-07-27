use clap::Parser;
mod cli;
mod executor;
mod scheduler;
mod server;

#[tokio::main]
async fn main() {
    let gflowd = cli::GFlowd::parse();
    env_logger::Builder::new()
        .filter_level(gflowd.verbose.log_level_filter())
        .init();

    match gflow::config::load_config(gflowd.config.as_ref()) {
        Ok(config) => {
            if let Err(e) = server::run(config).await {
                log::error!("Server failed: {}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    }
}
