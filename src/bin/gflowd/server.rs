use crate::scheduler::{self, SharedState};
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use gflow::{
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
        .route("/job", get(job_index).post(job_create).put(job_finish))
        .route("/info", get(info))
        .with_state(scheduler);
    let listener =
        tokio::net::TcpListener::bind(format!("localhost:{}", config.get_int("PORT").unwrap()))
            .await
            .unwrap();

    // --------clean by gflowd --cleanup --------
    let config_dir = get_config_temp_dir();
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).unwrap();
    }
    let gflowd_file = get_config_temp_file();
    std::fs::write(gflowd_file, config.get_int("PORT").unwrap().to_string()).unwrap();
    // ------------------------------------------

    log::info!("Listening on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[axum::debug_handler]
async fn info(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().await;
    let info = state.info();
    (StatusCode::OK, Json(info))
}

#[axum::debug_handler]
async fn job_index(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().await;
    let jobs = state.jobs.clone();
    (StatusCode::OK, Json(jobs))
}

#[axum::debug_handler]
async fn job_create(State(state): State<SharedState>, Json(input): Json<Job>) -> impl IntoResponse {
    let mut state = state.lock().await;
    log::info!("Received job: {:?}", input);
    state.submit_job(input);
    (StatusCode::CREATED, Json(()))
}

#[axum::debug_handler]
async fn job_finish(
    State(state): State<SharedState>,
    Json(input): Json<String>,
) -> impl IntoResponse {
    let mut state = state.lock().await;

    let job = state
        .jobs
        .iter_mut()
        .find(|j| j.run_name == Some(input.clone()));
    if let Some(j) = job {
        j.state = JobState::Finished;
    }
    log::info!("Finished job: {:?}", input);
    (StatusCode::OK, Json(()))
}
