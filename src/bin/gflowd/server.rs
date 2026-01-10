//! HTTP server for the gflow daemon
//!
//! # Security Note
//! The `/debug/*` endpoints expose full job details and per-user statistics without
//! authentication. In production environments, ensure the daemon is bound to localhost
//! only and protected by firewall rules. Consider gating these endpoints behind a
//! feature flag or configuration option for production deployments.

use crate::executor::TmuxExecutor;
use crate::scheduler_runtime::{self, SchedulerNotify, SharedState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use gflow::core::db::JobFilter;
use gflow::core::job::{Job, JobState};
use gflow::{debug, metrics};
use serde::Deserialize;
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;

/// Server state that includes both the scheduler and the notification handle
#[derive(Clone)]
struct ServerState {
    scheduler: SharedState,
    notify: SchedulerNotify,
}

#[derive(Deserialize)]
struct ListJobsQuery {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    state: Option<String>, // Comma-separated states
    #[serde(default)]
    user: Option<String>, // Comma-separated users
}

#[derive(serde::Serialize)]
struct PaginatedJobsResponse {
    jobs: Vec<Job>,
    total: usize,
    limit: usize,
    offset: usize,
}

pub async fn run(config: gflow::config::Config) -> anyhow::Result<()> {
    let state_dir = gflow::core::get_data_dir()?;
    let allowed_gpus = config.daemon.gpus.clone();

    // Inject TmuxExecutor
    let executor = Box::new(TmuxExecutor);

    let scheduler = Arc::new(tokio::sync::RwLock::new(
        scheduler_runtime::SchedulerRuntime::with_state_path(executor, state_dir, allowed_gpus)?,
    ));
    let scheduler_clone = Arc::clone(&scheduler);

    // Create notification handle for immediate scheduler wake-up
    let notify = Arc::new(Notify::new());
    let notify_clone = Arc::clone(&notify);

    tokio::spawn(async move {
        tracing::info!("Starting scheduler with immediate wake-up support...");
        scheduler_runtime::run(scheduler_clone, notify_clone).await;
    });

    // Create server state with both scheduler and notification handle
    let server_state = ServerState { scheduler, notify };

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/batch", post(create_jobs_batch))
        .route("/jobs/resolve-dependency", get(resolve_dependency))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/finish", post(finish_job))
        .route("/jobs/{id}/fail", post(fail_job))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/jobs/{id}/hold", post(hold_job))
        .route("/jobs/{id}/release", post(release_job))
        .route("/jobs/{id}/log", get(get_job_log))
        .route("/jobs/{id}/events", get(get_job_events))
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

    // Create shutdown signal handler
    let shutdown_signal = create_shutdown_signal();

    // Start Axum server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    tracing::info!("Server shutdown complete");
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
            tracing::info!("Received SIGTERM, initiating graceful shutdown");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT, initiating graceful shutdown");
        }
        _ = sigusr2.recv() => {
            tracing::info!("Received SIGUSR2 (reload signal), initiating graceful shutdown");
        }
    }
}

#[axum::debug_handler]
async fn info(State(server_state): State<ServerState>) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;
    let info = state.info();
    (StatusCode::OK, Json(info))
}

#[axum::debug_handler]
async fn list_jobs(
    State(server_state): State<ServerState>,
    Query(query): Query<ListJobsQuery>,
) -> impl IntoResponse {
    // If no query parameters, return jobs from memory (backward compatibility)
    if query.limit.is_none() && query.state.is_none() && query.user.is_none() {
        let state = server_state.scheduler.read().await;
        let mut jobs: Vec<_> = state.jobs().values().cloned().collect();
        jobs.sort_by_key(|j| j.id);
        return (StatusCode::OK, Json(jobs)).into_response();
    }

    // Build filter from query parameters
    let mut filter = JobFilter::new();

    if let Some(state_str) = query.state {
        let states: Vec<JobState> = state_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !states.is_empty() {
            filter = filter.with_states(states);
        }
    }

    if let Some(user_str) = query.user {
        let users: Vec<String> = user_str.split(',').map(|s| s.trim().to_string()).collect();
        if !users.is_empty() {
            filter = filter.with_users(users);
        }
    }

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    // Query from database
    let db = {
        let state = server_state.scheduler.read().await;
        state.db.clone()
    };
    match db.query_jobs_paginated(&filter, limit, offset) {
        Ok((jobs, total)) => {
            let response = PaginatedJobsResponse {
                jobs,
                total,
                limit,
                offset,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to query jobs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to query jobs"})),
            )
                .into_response()
        }
    }
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
    let (job_id, run_name, job_to_save, next_job_id) = {
        let mut state = server_state.scheduler.write().await;

        // Validate that dependency job exists if specified
        if let Some(dep_id) = input.depends_on {
            if !state.jobs().contains_key(&dep_id) {
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

        let (job_id, run_name, job_clone) = state.submit_job(input).await;
        let next_job_id = state.next_job_id();
        (job_id, run_name, job_clone, next_job_id)
    }; // Lock released here

    let _submitted_by = job_to_save.submitted_by.clone();

    // Persist without holding lock (using async writer)
    let writer = {
        let state = server_state.scheduler.read().await;
        state.writer.clone()
    };
    if let Err(e) = writer.insert_job(job_to_save.clone()).await {
        tracing::error!("Failed to insert job {} to database: {}", job_id, e);
    }
    writer.set_metadata("next_job_id".to_string(), next_job_id.to_string());

    // Log job creation event
    let event = gflow::core::job::JobEvent::created(job_id, job_to_save.state);
    writer.queue_event(event);

    // Notify scheduler immediately to avoid waiting for next 5-second interval
    server_state.notify.notify_one();

    // Record metrics
    #[cfg(feature = "metrics")]
    metrics::JOB_SUBMISSIONS
        .with_label_values(&[&_submitted_by])
        .inc();

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
    let (results, jobs_to_save, next_job_id) = {
        let mut state = server_state.scheduler.write().await;

        // Validate all dependencies exist before submitting any (fail-fast)
        for job in &input {
            if let Some(dep_id) = job.depends_on {
                if !state.jobs().contains_key(&dep_id) {
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
        }

        state.submit_jobs(input).await
    }; // Lock released here

    // Persist without holding lock (using async writer)
    let writer = {
        let state = server_state.scheduler.read().await;
        state.writer.clone()
    };
    if let Err(e) = writer.insert_jobs_batch(jobs_to_save.clone()).await {
        tracing::error!("Failed to batch insert jobs: {}", e);
    }
    writer.set_metadata("next_job_id".to_string(), next_job_id.to_string());

    // Log creation events for all jobs
    for job in &jobs_to_save {
        let event = gflow::core::job::JobEvent::created(job.id, job.state);
        writer.queue_event(event);
    }

    // Notify scheduler immediately to avoid waiting for next 5-second interval
    server_state.notify.notify_one();

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
    // First check memory (for active jobs)
    let state = server_state.scheduler.read().await;
    if let Some(job) = state.jobs().get(&id).cloned() {
        return Ok(Json(job));
    }

    // If not in memory, query database (for completed jobs)
    let db = state.db.clone();
    drop(state); // Release lock before database query

    match db.get_job(id) {
        Ok(Some(job)) => Ok(Json(job)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to query job {} from database: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[axum::debug_handler]
async fn finish_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Finishing job");

    // Get user before finishing (for metrics)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.jobs().get(&id).map(|j| j.submitted_by.clone())
    };

    let (success, job_to_save, writer) = {
        let mut state = server_state.scheduler.write().await;
        let success = state.finish_job(id).await;
        let job_to_save = if success {
            state.jobs().get(&id).cloned()
        } else {
            None
        };
        (success, job_to_save, state.writer.clone())
    }; // Lock released here

    // Save without holding lock (using async writer)
    if let Some(job) = job_to_save {
        let event = gflow::core::job::JobEvent::state_transition(
            id,
            JobState::Running,
            JobState::Finished,
            None,
        );
        writer.queue_update_with_event(job, event);

        // Notify scheduler immediately since dependent jobs may be ready to run
        server_state.notify.notify_one();
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
async fn get_job_events(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let db = {
        let state = server_state.scheduler.read().await;
        state.db.clone()
    };

    match db.get_job_events(id) {
        Ok(events) => (StatusCode::OK, Json(events)),
        Err(e) => {
            tracing::error!("Failed to get events for job {}: {}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(vec![]))
        }
    }
}

#[axum::debug_handler]
async fn fail_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Failing job");

    // Get user before failing (for metrics)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.jobs().get(&id).map(|j| j.submitted_by.clone())
    };

    let (success, job_to_save, writer) = {
        let mut state = server_state.scheduler.write().await;
        let success = state.fail_job(id).await;
        let job_to_save = if success {
            state.jobs().get(&id).cloned()
        } else {
            None
        };
        (success, job_to_save, state.writer.clone())
    }; // Lock released here

    // Save without holding lock (using async writer)
    if let Some(job) = job_to_save {
        let event = gflow::core::job::JobEvent::state_transition(
            id,
            JobState::Running,
            JobState::Failed,
            Some("Job marked as failed".to_string()),
        );
        writer.queue_update_with_event(job, event);
    }

    // Record metrics only on successful transition
    #[cfg(feature = "metrics")]
    if success {
        if let Some(submitted_by) = user {
            metrics::JOB_FAILED
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
async fn cancel_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Cancelling job");

    // Get user before cancelling (for metrics)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.jobs().get(&id).map(|j| j.submitted_by.clone())
    };

    let (success, job_to_save, old_state, writer) = {
        let mut state = server_state.scheduler.write().await;
        let old_state = state.jobs().get(&id).map(|j| j.state);
        let success = state.cancel_job(id).await;
        let job_to_save = if success {
            state.jobs().get(&id).cloned()
        } else {
            None
        };
        (success, job_to_save, old_state, state.writer.clone())
    }; // Lock released here

    // Save without holding lock (using async writer)
    if let Some(job) = job_to_save {
        if let Some(old) = old_state {
            let event = gflow::core::job::JobEvent::state_transition(
                id,
                old,
                JobState::Cancelled,
                Some("Job cancelled by user".to_string()),
            );
            writer.queue_update_with_event(job, event);
        }
    }

    // Record metrics only on successful transition
    #[cfg(feature = "metrics")]
    if success {
        if let Some(submitted_by) = user {
            metrics::JOB_CANCELLED
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
async fn hold_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::info!(job_id = id, "Holding job");

    let (success, job_to_save, writer) = {
        let mut state = server_state.scheduler.write().await;
        let success = state.hold_job(id).await;
        let job_to_save = if success {
            state.jobs().get(&id).cloned()
        } else {
            None
        };
        (success, job_to_save, state.writer.clone())
    }; // Lock released here

    // Save without holding lock (using async writer)
    if let Some(job) = job_to_save {
        let event = gflow::core::job::JobEvent::hold(id, Some("Job held by user".to_string()));
        writer.queue_update_with_event(job, event);
    }

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

    let (success, job_to_save, writer) = {
        let mut state = server_state.scheduler.write().await;
        let success = state.release_job(id).await;
        let job_to_save = if success {
            state.jobs().get(&id).cloned()
        } else {
            None
        };
        (success, job_to_save, state.writer.clone())
    }; // Lock released here

    // Save without holding lock (using async writer)
    if let Some(job) = job_to_save {
        let event =
            gflow::core::job::JobEvent::release(id, Some("Job released by user".to_string()));
        writer.queue_update_with_event(job, event);

        // Notify scheduler immediately since released job may be ready to run
        server_state.notify.notify_one();
    }

    if success {
        (StatusCode::OK, Json(()))
    } else {
        (StatusCode::NOT_FOUND, Json(()))
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

    // Save GPU metadata asynchronously
    let writer = state.writer.clone();
    if let Some(ref indices) = request.allowed_indices {
        if let Ok(json) = serde_json::to_string(indices) {
            writer.set_metadata("allowed_gpu_indices".to_string(), json);
        }
    } else {
        // Clear the metadata if no indices specified
        writer.set_metadata("allowed_gpu_indices".to_string(), "null".to_string());
    }

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

    let (updated_jobs, writer) = {
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

        (updated_jobs, state.writer.clone())
    }; // Lock released here

    // Save all updated jobs asynchronously
    for job in updated_jobs.iter() {
        writer.queue_update(job.clone());
    }

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
