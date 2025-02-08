use axum::{routing::get, Router};
use clap::Parser;
use config::load_config;
use scheduler::Scheduler;
use std::sync::Arc;
mod cli;
mod config;
mod job;
mod scheduler;

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
    let scheduler = Arc::new(Scheduler::new());

    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = tokio::net::TcpListener::bind(format!(
        "0.0.0.0:{}",
        config.unwrap().get_int("PORT").unwrap()
    ))
    .await
    .unwrap();
    axum::serve(listener, app).await.unwrap();
}
