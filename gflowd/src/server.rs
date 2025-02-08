use axum::{routing::get, Router};

use crate::scheduler::Scheduler;
use std::sync::Arc;

pub async fn run(config: config::Config) {
    let scheduler = Arc::new(Scheduler::new());

    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener =
        tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.get_int("PORT").unwrap()))
            .await
            .unwrap();
    axum::serve(listener, app).await.unwrap();
}
