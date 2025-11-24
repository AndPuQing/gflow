use crate::scheduler::{self, SharedState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use gflow::core::job::Job;
use std::sync::Arc;

pub async fn run(config: gflow::config::Config) -> anyhow::Result<()> {
    let scheduler = SharedState::default();
    let scheduler_clone = Arc::clone(&scheduler);

    tokio::spawn(async move {
        log::info!("Starting scheduler...");
        scheduler::run(scheduler_clone).await;
    });

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/resolve-dependency", get(resolve_dependency))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/finish", post(finish_job))
        .route("/jobs/{id}/fail", post(fail_job))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/jobs/{id}/hold", post(hold_job))
        .route("/jobs/{id}/release", post(release_job))
        .route("/jobs/{id}/log", get(get_job_log))
        .route("/info", get(info))
        .route("/health", get(get_health))
        .with_state(scheduler);
    let host = &config.daemon.host;
    let port = config.daemon.port;
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log::info!("Listening on: {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[axum::debug_handler]
async fn info(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.read().await;
    let info = state.info();
    (StatusCode::OK, Json(info))
}

#[axum::debug_handler]
async fn list_jobs(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.read().await;
    let mut jobs: Vec<_> = state.jobs.values().cloned().collect();
    jobs.sort_by_key(|j| j.id);
    (StatusCode::OK, Json(jobs))
}

#[axum::debug_handler]
async fn create_job(State(state): State<SharedState>, Json(input): Json<Job>) -> impl IntoResponse {
    let mut state = state.write().await;
    log::info!("Received job: {input:?}");

    // Validate that dependency job exists if specified
    if let Some(dep_id) = input.depends_on {
        if !state.jobs.contains_key(&dep_id) {
            log::warn!(
                "Job submission failed: dependency job {} does not exist",
                dep_id
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Dependency job {} does not exist", dep_id)
                })),
            );
        }
    }

    let (job_id, run_name) = state.submit_job(input).await;
    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": job_id, "run_name": run_name })),
    )
}

#[axum::debug_handler]
async fn get_job(
    State(state): State<SharedState>,
    Path(id): Path<u32>,
) -> Result<Json<Job>, StatusCode> {
    let state = state.read().await;
    state
        .jobs
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[axum::debug_handler]
async fn finish_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let mut state = state.write().await;
    log::info!("Finishing job with ID: {id}");
    if state.finish_job(id).await {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn get_job_log(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let state = state.read().await;
    if state.jobs.contains_key(&id) {
        match gflow::core::get_log_file_path(id) {
            Ok(path) => {
                if path.exists() {
                    (StatusCode::OK, Json(Some(path)))
                } else {
                    (StatusCode::NOT_FOUND, Json(None))
                }
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(None)),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

#[axum::debug_handler]
async fn fail_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let mut state = state.write().await;
    log::info!("Failing job with ID: {id}");
    if state.fail_job(id).await {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn cancel_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let mut state = state.write().await;
    log::info!("Cancelling job with ID: {id}");
    if state.cancel_job(id).await {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn hold_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let mut state = state.write().await;
    log::info!("Holding job with ID: {id}");
    if state.hold_job(id).await {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn release_job(State(state): State<SharedState>, Path(id): Path<u32>) -> impl IntoResponse {
    let mut state = state.write().await;
    log::info!("Releasing job with ID: {id}");
    if state.release_job(id).await {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn resolve_dependency(
    State(state): State<SharedState>,
    axum::extract::Query(params): axum::extract::Query<ResolveDependencyQuery>,
) -> impl IntoResponse {
    let state = state.read().await;

    if let Some(resolved_id) = state.resolve_dependency(&params.username, &params.shorthand) {
        (
            StatusCode::OK,
            Json(serde_json::json!({ "job_id": resolved_id })),
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Cannot resolve dependency '{}' for user '{}'", params.shorthand, params.username)
            })),
        )
    }
}

#[derive(serde::Deserialize)]
struct ResolveDependencyQuery {
    username: String,
    shorthand: String,
}

#[axum::debug_handler]
async fn get_health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}
