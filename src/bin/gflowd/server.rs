use crate::executor::TmuxExecutor;
use crate::scheduler_runtime::{self, SharedState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use gflow::core::job::Job;
use socket2::{Domain, Protocol, Socket, Type};
use std::sync::Arc;

pub async fn run(config: gflow::config::Config) -> anyhow::Result<()> {
    let state_path = gflow::core::get_data_dir()?.join("state.json");
    let allowed_gpus = config.daemon.gpus.clone();

    // Inject TmuxExecutor
    let executor = Box::new(TmuxExecutor);

    let scheduler = Arc::new(tokio::sync::RwLock::new(
        scheduler_runtime::SchedulerRuntime::with_state_path(executor, state_path, allowed_gpus)?,
    ));
    let scheduler_clone = Arc::clone(&scheduler);

    tokio::spawn(async move {
        log::info!("Starting scheduler...");
        scheduler_runtime::run(scheduler_clone).await;
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
        .route("/gpus", post(set_allowed_gpus))
        .with_state(scheduler);

    // Create socket with SO_REUSEPORT for hot reload support
    let host = &config.daemon.host;
    let port = config.daemon.port;
    let bind_addr = format!("{host}:{port}");

    // Resolve hostname to socket address (supports "localhost", IPv4, and IPv6)
    let addr = tokio::net::lookup_host(&bind_addr)
        .await?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve address: {}", bind_addr))?;

    // Determine domain from resolved address
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?; // Enable SO_REUSEPORT for hot reload
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;

    // Convert to tokio TcpListener
    let std_listener: std::net::TcpListener = socket.into();
    std_listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;

    log::info!("Listening on: {addr} (SO_REUSEPORT enabled)");

    // Create shutdown signal handler
    let shutdown_signal = create_shutdown_signal();

    // Start Axum server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    log::info!("Server shutdown complete");
    Ok(())
}

async fn create_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
    let mut sigusr2 =
        signal(SignalKind::user_defined2()).expect("Failed to register SIGUSR2 handler");

    tokio::select! {
        _ = sigterm.recv() => {
            log::info!("Received SIGTERM, initiating graceful shutdown");
        }
        _ = sigint.recv() => {
            log::info!("Received SIGINT, initiating graceful shutdown");
        }
        _ = sigusr2.recv() => {
            log::info!("Received SIGUSR2 (reload signal), initiating graceful shutdown");
        }
    }
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
    let mut jobs: Vec<_> = state.jobs().values().cloned().collect();
    jobs.sort_by_key(|j| j.id);
    (StatusCode::OK, Json(jobs))
}

#[axum::debug_handler]
async fn create_job(State(state): State<SharedState>, Json(input): Json<Job>) -> impl IntoResponse {
    let mut state = state.write().await;
    log::info!("Received job: {input:?}");
    log::info!(
        "Job group_id: {:?}, max_concurrent: {:?}",
        input.group_id,
        input.max_concurrent
    );

    // Validate that dependency job exists if specified
    if let Some(dep_id) = input.depends_on {
        if !state.jobs().contains_key(&dep_id) {
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
        .jobs()
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
    if state.jobs().contains_key(&id) {
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

#[derive(serde::Deserialize)]
struct SetGpusRequest {
    allowed_indices: Option<Vec<u32>>,
}

#[axum::debug_handler]
async fn set_allowed_gpus(
    State(state): State<SharedState>,
    Json(request): Json<SetGpusRequest>,
) -> impl IntoResponse {
    let mut state = state.write().await;

    // Validate GPU indices
    let detected_count = state.gpu_slots_count();
    if let Some(ref allowed) = request.allowed_indices {
        let invalid: Vec<_> = allowed
            .iter()
            .filter(|&&idx| idx >= detected_count as u32)
            .copied()
            .collect();

        if !invalid.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!(
                        "Invalid GPU indices {:?} (only {} GPUs detected)",
                        invalid, detected_count
                    )
                })),
            );
        }
    }

    state.set_allowed_gpu_indices(request.allowed_indices.clone());
    state.save_state().await;

    log::info!("GPU configuration updated: {:?}", request.allowed_indices);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "allowed_gpu_indices": request.allowed_indices
        })),
    )
}
