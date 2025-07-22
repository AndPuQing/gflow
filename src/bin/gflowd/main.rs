use clap::Parser;
use config::load_config;
use gflow::core::get_config_temp_file;
mod cli;
mod config;
mod executor;
mod scheduler;
mod server;

#[tokio::main]
async fn main() {
    let gflowd = cli::GFlowd::parse();
    env_logger::Builder::new()
        .filter_level(gflowd.verbose.log_level_filter())
        .init();
    log::debug!("Parsed CLI arguments: {:?}", gflowd);

    if gflowd.cleanup {
        let gflowd_file = get_config_temp_file();
        if gflowd_file.exists() {
            std::fs::remove_file(gflowd_file).ok();
        }
        std::process::exit(0);
    }

    match load_config(gflowd) {
        Ok(config) => server::run(config).await,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    }
}
