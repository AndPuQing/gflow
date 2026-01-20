//! HTTP server for the gflow daemon
//!
//! # Security Note
//! The `/debug/*` endpoints expose full job details and per-user statistics without
//! authentication. In production environments, ensure the daemon is bound to localhost
//! only and protected by firewall rules. Consider gating these endpoints behind a
//! feature flag or configuration option for production deployments.

use crate::events::{EventBus, SchedulerEvent};
use crate::executor::TmuxExecutor;
use crate::scheduler_runtime::{self, SharedState};
use crate::state_saver::StateSaverHandle;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use gflow::core::job::{Job, JobState};
use gflow::{debug, metrics};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Server state that includes both the scheduler and the event bus
#[derive(Clone)]
struct ServerState {
    scheduler: SharedState,
    event_bus: Arc<EventBus>,
    _state_saver: StateSaverHandle,
}

pub async fn run(config: gflow::config::Config) -> anyhow::Result<()> {
    let state_dir = gflow::core::get_data_dir()?;
    let allowed_gpus = config.daemon.gpus.clone();

    // Inject TmuxExecutor
    let executor = Box::new(TmuxExecutor);

    // Create state saver channel before initializing SchedulerRuntime
    let (state_tx, state_rx) = tokio::sync::mpsc::unbounded_channel();
    let state_saver_handle = StateSaverHandle::new(state_tx);

    // Create SchedulerRuntime and set state saver
    let mut scheduler_runtime =
        scheduler_runtime::SchedulerRuntime::with_state_path(executor, state_dir, allowed_gpus)?;
    scheduler_runtime.set_state_saver(state_saver_handle.clone());

    let scheduler = Arc::new(tokio::sync::RwLock::new(scheduler_runtime));
    let scheduler_clone = Arc::clone(&scheduler);

    // Create event bus for event-driven scheduling
    let event_bus = Arc::new(EventBus::new(1000));
    let event_bus_clone = Arc::clone(&event_bus);

    // Spawn state saver task (30 second interval)
    let scheduler_for_saver = Arc::clone(&scheduler);
    let state_saver_task = tokio::spawn(async move {
        tracing::info!("Starting state saver task with 30s interval...");
        crate::state_saver::run(scheduler_for_saver, state_rx, Duration::from_secs(30)).await;
    });
    state_saver_handle.set_task_handle(state_saver_task);

    // Spawn event-driven scheduler task
    tokio::spawn(async move {
        tracing::info!("Starting event-driven scheduler...");
        scheduler_runtime::run_event_driven(scheduler_clone, event_bus_clone).await;
    });

    // Create server state with scheduler, event bus, and state saver
    let server_state = ServerState {
        scheduler,
        event_bus,
        _state_saver: state_saver_handle.clone(),
    };

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/batch", post(create_jobs_batch))
        .route("/jobs/resolve-dependency", get(resolve_dependency))
        .route("/jobs/{id}", get(get_job).patch(update_job))
        .route("/jobs/{id}/finish", post(finish_job))
        .route("/jobs/{id}/fail", post(fail_job))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/jobs/{id}/hold", post(hold_job))
        .route("/jobs/{id}/release", post(release_job))
        .route("/jobs/{id}/log", get(get_job_log))
        .route("/info", get(info))
        .route("/health", get(get_health))
        .route("/gpus", post(set_allowed_gpus))
        .route(
            "/groups/{group_id}/max-concurrency",
            post(set_group_max_concurrency),
        )
        .route("/metrics", get(get_metrics))
        .route("/debug/state", get(debug_state))
        .route("/debug/jobs/{id}", get(debug_job))
        .route("/debug/metrics", get(debug_metrics))
        .with_state(server_state);

    // Create socket with SO_REUSEPORT for hot reload support
    let host = &config.daemon.host;
    let port = config.daemon.port;

    // Handle IPv6 literal addresses (e.g., "::1" -> "[::1]")
    let bind_addr = if host.contains(':') && !host.starts_with('[') {
        // IPv6 literal without brackets
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };

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

    tracing::info!("Listening on: {addr} (SO_REUSEPORT enabled)");

    // Create shutdown signal handler with state saver for graceful shutdown
    let shutdown_signal = create_shutdown_signal(state_saver_handle);

    // Start Axum server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    tracing::info!("Server shutdown complete");
    Ok(())
}

async fn create_shutdown_signal(state_saver: StateSaverHandle) {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
    let mut sigusr2 =
        signal(SignalKind::user_defined2()).expect("Failed to register SIGUSR2 handler");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT, initiating graceful shutdown");
        }
        _ = sigusr2.recv() => {
            tracing::info!("Received SIGUSR2 (reload signal), initiating graceful shutdown");
        }
    }

    // Save state before exiting
    tracing::info!("Saving state before shutdown...");
    if let Err(e) = state_saver.shutdown_and_wait().await {
        tracing::error!("Failed to save state during shutdown: {}", e);
    } else {
        tracing::info!("State saved successfully");
    }
}

#[axum::debug_handler]
async fn info(State(server_state): State<ServerState>) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;
    let info = state.info();
    (StatusCode::OK, Json(info))
}

#[derive(serde::Deserialize)]
struct ListJobsQuery {
    state: Option<String>,
    user: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    created_after: Option<i64>,
}

#[axum::debug_handler]
async fn list_jobs(
    State(server_state): State<ServerState>,
    axum::extract::Query(params): axum::extract::Query<ListJobsQuery>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;
    let mut jobs: Vec<_> = state.jobs().values().cloned().collect();

    // Apply state filter if provided
    if let Some(states_str) = params.state {
        let states: Vec<JobState> = states_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !states.is_empty() {
            jobs.retain(|job| states.contains(&job.state));
        }
    }

    // Apply user filter if provided
    if let Some(users_str) = params.user {
        let users: Vec<String> = users_str.split(',').map(|s| s.trim().to_string()).collect();
        if !users.is_empty() {
            jobs.retain(|job| users.contains(&job.submitted_by));
        }
    }

    // Apply created_after filter if provided
    if let Some(secs) = params.created_after {
        use std::time::{Duration, UNIX_EPOCH};

        if let Some(created_after) = UNIX_EPOCH.checked_add(Duration::from_secs(secs.max(0) as u64))
        {
            jobs.retain(|job| job.started_at.is_some_and(|ts| ts >= created_after));
        }
    }

    // Sort by job ID
    jobs.sort_by_key(|j| j.id);

    // Apply pagination if provided
    let total = jobs.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(total);

    let jobs: Vec<_> = jobs.into_iter().skip(offset).take(limit).collect();

    (StatusCode::OK, Json(jobs))
}

#[axum::debug_handler]
async fn create_job(
    State(server_state): State<ServerState>,
    Json(input): Json<Job>,
) -> impl IntoResponse {
    tracing::info!(
        user = %input.submitted_by,
        gpus = input.gpus,
        group_id = ?input.group_id,
        max_concurrent = ?input.max_concurrent,
        "Received job submission"
    );

    // Validate dependency and submit job
    let (job_id, run_name) = {
        let mut state = server_state.scheduler.write().await;

        // Collect all dependencies (legacy + new)
        let mut all_deps = input.depends_on_ids.clone();
        if let Some(dep) = input.depends_on {
            if !all_deps.contains(&dep) {
                all_deps.push(dep);
            }
        }

        // Validate all dependencies exist
        for dep_id in &all_deps {
            if !state.jobs().contains_key(dep_id) {
                tracing::warn!(
                    dep_id = dep_id,
                    "Job submission failed: dependency job does not exist"
                );
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Dependency job {} does not exist", dep_id)
                    })),
                );
            }
        }

        // Check for circular dependencies
        let next_id = state.next_job_id();
        if let Err(cycle_msg) = state.validate_no_circular_dependency(next_id, &all_deps) {
            tracing::warn!("Circular dependency detected: {}", cycle_msg);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": cycle_msg
                })),
            );
        }

        let (job_id, run_name, _job_clone) = state.submit_job(input).await;
        (job_id, run_name)
    }; // Lock released here

    // Publish JobSubmitted event to trigger scheduling
    server_state
        .event_bus
        .publish(SchedulerEvent::JobSubmitted { job_id });

    // Record metrics
    #[cfg(feature = "metrics")]
    {
        let state = server_state.scheduler.read().await;
        if let Some(job) = state.jobs().get(&job_id) {
            metrics::JOB_SUBMISSIONS
                .with_label_values(&[&job.submitted_by])
                .inc();
        }
    }

    tracing::info!(job_id = job_id, run_name = %run_name, "Job created");

    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": job_id, "run_name": run_name })),
    )
}

#[axum::debug_handler]
async fn create_jobs_batch(
    State(server_state): State<ServerState>,
    Json(input): Json<Vec<Job>>,
) -> impl IntoResponse {
    if input.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Batch must contain at least one job"})),
        );
    }

    if input.len() > 1000 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "Batch size exceeds maximum of 1000 jobs"})),
        );
    }

    tracing::info!(count = input.len(), "Received batch job submission");

    // Validate and submit jobs
    let (results, _jobs_to_save, _next_job_id) = {
        let mut state = server_state.scheduler.write().await;

        // Validate all dependencies exist before submitting any (fail-fast)
        for job in &input {
            // Collect all dependencies (legacy + new)
            let mut all_deps = job.depends_on_ids.clone();
            if let Some(dep) = job.depends_on {
                if !all_deps.contains(&dep) {
                    all_deps.push(dep);
                }
            }

            // Validate all dependencies exist
            for dep_id in &all_deps {
                if !state.jobs().contains_key(dep_id) {
                    tracing::warn!(
                        dep_id = dep_id,
                        "Batch job submission failed: dependency job does not exist"
                    );
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Dependency job {} does not exist", dep_id)
                        })),
                    );
                }
            }

            // Check for circular dependencies
            let next_id = state.next_job_id();
            if let Err(cycle_msg) = state.validate_no_circular_dependency(next_id, &all_deps) {
                tracing::warn!("Circular dependency detected: {}", cycle_msg);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": cycle_msg
                    })),
                );
            }
        }

        state.submit_jobs(input).await
    }; // Lock released here

    // Publish JobSubmitted events for all submitted jobs
    for (job_id, _, _) in &results {
        server_state
            .event_bus
            .publish(SchedulerEvent::JobSubmitted { job_id: *job_id });
    }

    // Record metrics
    #[cfg(feature = "metrics")]
    for (_, _, submitted_by) in &results {
        metrics::JOB_SUBMISSIONS
            .with_label_values(&[submitted_by])
            .inc();
    }

    tracing::info!(count = results.len(), "Batch jobs created");

    let response: Vec<_> = results
        .into_iter()
        .map(|(job_id, run_name, _)| {
            serde_json::json!({
                "id": job_id,
                "run_name": run_name
            })
        })
        .collect();

    (
        StatusCode::CREATED,
        Json(serde_json::Value::Array(response)),
    )
}

#[axum::debug_handler]
async fn get_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Result<Json<Job>, StatusCode> {
    let state = server_state.scheduler.read().await;
    state
        .jobs()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[axum::debug_handler]
async fn finish_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Finishing job");

    // Get job info before finishing (for metrics and events)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.jobs().get(&id).map(|j| j.submitted_by.clone())
    };

    let (success, gpu_ids, memory_mb) = {
        let mut state = server_state.scheduler.write().await;
        let job_info = state
            .jobs()
            .get(&id)
            .map(|j| (j.gpu_ids.clone(), j.memory_limit_mb));
        let success = state.finish_job(id).await;
        if let Some((gpu_ids, memory_mb)) = job_info {
            (success, gpu_ids, memory_mb)
        } else {
            (success, None, None)
        }
    }; // Lock released here

    if success {
        // Publish JobCompleted event to trigger scheduling and cascade
        server_state
            .event_bus
            .publish(SchedulerEvent::JobCompleted {
                job_id: id,
                final_state: JobState::Finished,
                gpu_ids,
                memory_mb,
            });
    }

    // Record metrics only on successful transition
    #[cfg(feature = "metrics")]
    if success {
        if let Some(submitted_by) = user {
            metrics::JOB_FINISHED
                .with_label_values(&[&submitted_by])
                .inc();
        }
    }

    if success {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn get_job_log(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    // Check if job exists in memory
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
async fn fail_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Failing job");

    // Get user and job info before failing (for metrics and events)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.jobs().get(&id).map(|j| j.submitted_by.clone())
    };

    let (_success, gpu_ids, memory_mb) = {
        let state = server_state.scheduler.read().await;
        if let Some(job) = state.jobs().get(&id) {
            (true, job.gpu_ids.clone(), job.memory_limit_mb)
        } else {
            (false, None, None)
        }
    };

    let result = {
        let mut state = server_state.scheduler.write().await;
        state.fail_job(id).await
    }; // Lock released here

    if result {
        // Publish JobCompleted event to trigger cascade cancellation
        server_state
            .event_bus
            .publish(SchedulerEvent::JobCompleted {
                job_id: id,
                final_state: JobState::Failed,
                gpu_ids,
                memory_mb,
            });
    }

    // Record metrics only on successful transition
    #[cfg(feature = "metrics")]
    if result {
        if let Some(submitted_by) = user {
            metrics::JOB_FAILED
                .with_label_values(&[&submitted_by])
                .inc();
        }
    }

    if result {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn cancel_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Cancelling job");

    // Get user and job info before cancelling (for metrics and events)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.jobs().get(&id).map(|j| j.submitted_by.clone())
    };

    let (_success, gpu_ids, memory_mb) = {
        let state = server_state.scheduler.read().await;
        if let Some(job) = state.jobs().get(&id) {
            (true, job.gpu_ids.clone(), job.memory_limit_mb)
        } else {
            (false, None, None)
        }
    };

    let result = {
        let mut state = server_state.scheduler.write().await;
        state.cancel_job(id).await
    }; // Lock released here

    if result {
        // Publish JobCompleted event to trigger cascade cancellation
        server_state
            .event_bus
            .publish(SchedulerEvent::JobCompleted {
                job_id: id,
                final_state: JobState::Cancelled,
                gpu_ids,
                memory_mb,
            });
    }

    // Record metrics only on successful transition
    #[cfg(feature = "metrics")]
    if result {
        if let Some(submitted_by) = user {
            metrics::JOB_CANCELLED
                .with_label_values(&[&submitted_by])
                .inc();
        }
    }

    if result {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn hold_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Holding job");

    let success = {
        let mut state = server_state.scheduler.write().await;
        state.hold_job(id).await
    }; // Lock released here

    if success {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn release_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Releasing job");

    let success = {
        let mut state = server_state.scheduler.write().await;
        state.release_job(id).await
    }; // Lock released here

    if success {
        // Publish JobSubmitted event since released job may be ready to run
        server_state
            .event_bus
            .publish(SchedulerEvent::JobSubmitted { job_id: id });
    }

    if success {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
    }
}

#[axum::debug_handler]
async fn update_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
    Json(request): Json<UpdateJobRequest>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Updating job parameters");

    let result = {
        let mut state = server_state.scheduler.write().await;
        state.update_job(id, request).await
    }; // Lock released here

    match result {
        Ok((job, updated_fields)) => {
            tracing::info!(
                job_id = id,
                updated_fields = ?updated_fields,
                "Job updated successfully"
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "job": job,
                    "updated_fields": updated_fields,
                })),
            )
        }
        Err(error) => {
            tracing::error!(job_id = id, error = %error, "Failed to update job");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": error,
                })),
            )
        }
    }
}

#[axum::debug_handler]
async fn resolve_dependency(
    State(server_state): State<ServerState>,
    axum::extract::Query(params): axum::extract::Query<ResolveDependencyQuery>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

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
    let pid = std::process::id();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ok", "pid": pid })),
    )
}

#[derive(serde::Deserialize)]
struct SetGpusRequest {
    allowed_indices: Option<Vec<u32>>,
}

#[axum::debug_handler]
async fn set_allowed_gpus(
    State(server_state): State<ServerState>,
    Json(request): Json<SetGpusRequest>,
) -> impl IntoResponse {
    let mut state = server_state.scheduler.write().await;

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

    tracing::info!(allowed_indices = ?request.allowed_indices, "GPU configuration updated");

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "allowed_gpu_indices": request.allowed_indices
        })),
    )
}

#[derive(serde::Deserialize)]
struct SetGroupMaxConcurrencyRequest {
    max_concurrent: usize,
}

#[derive(serde::Deserialize)]
pub struct UpdateJobRequest {
    pub command: Option<String>,
    pub script: Option<std::path::PathBuf>,
    pub gpus: Option<u32>,
    pub conda_env: Option<Option<String>>, // Nested Option to allow clearing
    pub priority: Option<u8>,
    pub parameters: Option<HashMap<String, String>>,
    pub time_limit: Option<Option<std::time::Duration>>,
    pub memory_limit_mb: Option<Option<u64>>,
    pub depends_on_ids: Option<Vec<u32>>,
    pub dependency_mode: Option<Option<gflow::core::job::DependencyMode>>,
    pub auto_cancel_on_dependency_failure: Option<bool>,
    pub max_concurrent: Option<Option<usize>>,
}

#[axum::debug_handler]
async fn set_group_max_concurrency(
    State(server_state): State<ServerState>,
    Path(group_id): Path<String>,
    Json(request): Json<SetGroupMaxConcurrencyRequest>,
) -> impl IntoResponse {
    tracing::info!(
        group_id = %group_id,
        max_concurrent = request.max_concurrent,
        "Setting group max_concurrency"
    );

    let updated_jobs = {
        let mut state = server_state.scheduler.write().await;

        // Find all jobs in this group and collect their IDs
        let job_ids: Vec<u32> = state
            .jobs()
            .iter()
            .filter(|(_, job)| job.group_id.as_ref() == Some(&group_id))
            .map(|(id, _)| *id)
            .collect();

        if job_ids.is_empty() {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("No jobs found with group_id '{}'", group_id)
                })),
            );
        }

        // Update max_concurrent for all jobs in the group
        let mut updated_jobs = Vec::new();
        for job_id in job_ids {
            if let Some(job) = state.update_job_max_concurrent(job_id, request.max_concurrent) {
                updated_jobs.push(job);
            }
        }

        updated_jobs
    }; // Lock released here

    tracing::info!(
        group_id = %group_id,
        updated_count = updated_jobs.len(),
        "Group max_concurrency updated"
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "group_id": group_id,
            "max_concurrent": request.max_concurrent,
            "updated_jobs": updated_jobs.len()
        })),
    )
}

// Metrics endpoint
#[axum::debug_handler]
async fn get_metrics() -> impl IntoResponse {
    match metrics::export_metrics() {
        Ok(text) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4")],
            text,
        ),
        Err(e) => {
            tracing::error!("Failed to export metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "text/plain; version=0.0.4")],
                String::from("Error exporting metrics"),
            )
        }
    }
}

// Debug endpoints
#[axum::debug_handler]
async fn debug_state(State(server_state): State<ServerState>) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    // Get GPU info from the info() method
    let info = state.info();
    let gpu_slots: Vec<debug::DebugGpuSlot> = info
        .gpus
        .iter()
        .map(|gpu_info| debug::DebugGpuSlot {
            uuid: gpu_info.uuid.clone(),
            index: gpu_info.index,
            available: gpu_info.available,
        })
        .collect();

    let debug_state = debug::DebugState {
        jobs: state.jobs().clone(),
        next_job_id: state.next_job_id(),
        total_memory_mb: state.total_memory_mb(),
        available_memory_mb: state.available_memory_mb(),
        gpu_slots,
        allowed_gpu_indices: info.allowed_gpu_indices,
    };

    (StatusCode::OK, Json(debug_state))
}

#[axum::debug_handler]
async fn debug_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    state
        .jobs()
        .get(&id)
        .cloned()
        .map(debug::DebugJobInfo::from_job)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[axum::debug_handler]
async fn debug_metrics(State(server_state): State<ServerState>) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    let jobs_by_state: HashMap<JobState, usize> =
        state.jobs().values().fold(HashMap::new(), |mut acc, job| {
            *acc.entry(job.state).or_insert(0) += 1;
            acc
        });

    let jobs_by_user: HashMap<String, debug::UserJobStats> =
        state.jobs().values().fold(HashMap::new(), |mut acc, job| {
            let stats = acc
                .entry(job.submitted_by.clone())
                .or_insert(debug::UserJobStats {
                    submitted: 0,
                    running: 0,
                    finished: 0,
                    failed: 0,
                });
            stats.submitted += 1;
            match job.state {
                JobState::Running => stats.running += 1,
                JobState::Finished => stats.finished += 1,
                JobState::Failed => stats.failed += 1,
                _ => {}
            }
            acc
        });

    let debug_metrics = debug::DebugMetrics {
        jobs_by_state,
        jobs_by_user,
    };

    (StatusCode::OK, Json(debug_metrics))
}
