use crate::scheduler::{self, SharedState};
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use gflow_core::{
    get_config_temp_dir, get_config_temp_file,
    job::{Job, JobState},
};
use std::sync::Arc;

pub async fn run(config: config::Config) {
    let scheduler = SharedState::default();
    let scheduler_clone = Arc::clone(&scheduler);

    tokio::spawn(async move {
        log::info!("Starting scheduler...");
        scheduler::run(scheduler_clone).await;
    });

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/:id", get(get_job).patch(update_job))
        .route("/info", get(info))
        .with_state(scheduler);
    let port = config.get_int("PORT").unwrap_or(59000);
    let listener = tokio::net::TcpListener::bind(format!("localhost:{}", port))
        .await
        .expect("Failed to bind to port");

    // --------clean by gflowd --cleanup --------
    let config_dir = get_config_temp_dir();
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).ok();
    }
    let gflowd_file = get_config_temp_file();
    std::fs::write(gflowd_file, port.to_string()).ok();
    // ------------------------------------------

    if let Ok(addr) = listener.local_addr() {
        log::info!("Listening on: {}", addr);
    }
    axum::serve(listener, app).await.ok();
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
async fn get_job(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let state = state.lock().await;
    if let Some(job) = state.jobs.iter().find(|j| j.run_name == Some(id.clone())) {
        (StatusCode::OK, Json(Some(job.clone())))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

#[axum::debug_handler]
async fn update_job(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(input): Json<JobState>,
) -> impl IntoResponse {
    let mut state = state.lock().await;
    if let Some(job) = state
        .jobs
        .iter_mut()
        .find(|j| j.run_name == Some(id.clone()))
    {
        job.state = input;
        state.save_state();
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}
