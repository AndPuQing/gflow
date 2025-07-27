use crate::scheduler::{self, SharedState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use gflow::core::{get_config_temp_dir, get_config_temp_file, job::Job};
use std::sync::Arc;

pub async fn run(config: config::Config) -> anyhow::Result<()> {
    let scheduler = SharedState::default();
    let scheduler_clone = Arc::clone(&scheduler);

    tokio::spawn(async move {
        log::info!("Starting scheduler...");
        scheduler::run(scheduler_clone).await;
    });

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/:id", get(get_job))
        .route("/jobs/:id/finish", post(finish_job))
        .route("/jobs/:id/fail", post(fail_job))
        .route("/info", get(info))
        .with_state(scheduler);
    let port = config.get_int("PORT").unwrap_or(59000);
    let listener = tokio::net::TcpListener::bind(format!("localhost:{port}"))
        .await
        .expect("Failed to bind to port");

    // --------clean by gflowd --cleanup --------
    if let Ok(config_dir) = get_config_temp_dir() {
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir).ok();
        }
        if let Ok(gflowd_file) = get_config_temp_file() {
            std::fs::write(gflowd_file, port.to_string()).ok();
        }
    }
    // ------------------------------------------

    if let Ok(addr) = listener.local_addr() {
        log::info!("Listening on: {}", addr);
    }
    axum::serve(listener, app).await?;
    Ok(())
}

#[axum::debug_handler]
async fn info(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().await;
    let info = state.info();
    (StatusCode::OK, Json(info))
}

#[axum::debug_handler]
async fn list_jobs(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().await;
    let jobs = state.jobs.clone();
    (StatusCode::OK, Json(jobs))
}

#[axum::debug_handler]
async fn create_job(State(state): State<SharedState>, Json(input): Json<Job>) -> impl IntoResponse {
    let mut state = state.lock().await;
    log::info!("Received job: {:?}", input);
    state.submit_job(input);
    (StatusCode::CREATED, Json(()))
}

#[axum::debug_handler]
async fn get_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let state = state.lock().await;
    if let Some(job) = state.jobs.iter().find(|j| j.id == id) {
        (StatusCode::OK, Json(Some(job.clone())))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

#[axum::debug_handler]
async fn finish_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let mut state = state.lock().await;
    if state.finish_job(id) {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn fail_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let mut state = state.lock().await;
    if state.fail_job(id) {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}
