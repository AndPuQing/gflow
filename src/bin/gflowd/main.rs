use clap::Parser;
mod cli;
mod executor;
mod scheduler;
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let gflowd = cli::GFlowd::parse();
    env_logger::Builder::new()
        .filter_level(gflowd.verbose.log_level_filter())
        .init();

    let config = gflow::config::load_config(gflowd.config.as_ref())?;
    server::run(config).await
}
