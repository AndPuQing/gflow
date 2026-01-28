use super::super::state::{reject_if_read_only, ServerState};
use crate::events::SchedulerEvent;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use gflow::core::job::{Job, JobState};
use std::collections::HashMap;

#[axum::debug_handler]
pub(in crate::server) async fn info(State(server_state): State<ServerState>) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;
    let info = state.info();
    (StatusCode::OK, Json(info))
}

#[derive(serde::Deserialize)]
pub(in crate::server) struct ListJobsQuery {
    state: Option<String>,
    user: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    created_after: Option<i64>,
}

#[axum::debug_handler]
pub(in crate::server) async fn list_jobs(
    State(server_state): State<ServerState>,
    axum::extract::Query(params): axum::extract::Query<ListJobsQuery>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    // Parse filters once before iteration
    let state_filter: Option<Vec<JobState>> = params.state.as_ref().map(|states_str| {
        states_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    });

    let user_filter: Option<Vec<String>> = params
        .user
        .as_ref()
        .map(|users_str| users_str.split(',').map(|s| s.trim().to_string()).collect());

    let time_filter = params.created_after.and_then(|secs| {
        use std::time::{Duration, UNIX_EPOCH};
        UNIX_EPOCH.checked_add(Duration::from_secs(secs.max(0) as u64))
    });

    // Single-pass filter: iterate once and collect matching jobs
    let mut jobs: Vec<_> = state
        .jobs()
        .iter()
        .filter(|job| {
            // Apply state filter
            if let Some(ref states) = state_filter {
                if !states.is_empty() && !states.contains(&job.state) {
                    return false;
                }
            }

            // Apply user filter
            if let Some(ref users) = user_filter {
                if !users.is_empty() && !users.iter().any(|u| u == job.submitted_by.as_str()) {
                    return false;
                }
            }

            // Apply time filter
            if let Some(created_after) = time_filter {
                if job.submitted_at.is_none_or(|ts| ts < created_after) {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect();

    // Sort by job ID
    jobs.sort_unstable_by_key(|j| j.id);

    // Apply pagination
    let total = jobs.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(total);

    let jobs: Vec<_> = jobs.into_iter().skip(offset).take(limit).collect();

    (StatusCode::OK, Json(jobs))
}

#[axum::debug_handler]
pub(in crate::server) async fn create_job(
    State(server_state): State<ServerState>,
    Json(input): Json<Job>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
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
            if state.get_job(*dep_id).is_none() {
                tracing::warn!(
                    dep_id = dep_id,
                    "Job submission failed: dependency job does not exist"
                );
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Dependency job {} does not exist", dep_id)
                    })),
                )
                    .into_response();
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
            )
                .into_response();
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
        if let Some(job) = state.get_job(job_id) {
            gflow::metrics::JOB_SUBMISSIONS
                .with_label_values(&[&job.submitted_by])
                .inc();
        }
    }

    tracing::info!(job_id = job_id, run_name = %run_name, "Job created");

    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": job_id, "run_name": run_name })),
    )
        .into_response()
}

#[axum::debug_handler]
pub(in crate::server) async fn create_jobs_batch(
    State(server_state): State<ServerState>,
    Json(input): Json<Vec<Job>>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
    if input.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Batch must contain at least one job"})),
        )
            .into_response();
    }

    if input.len() > 1000 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "Batch size exceeds maximum of 1000 jobs"})),
        )
            .into_response();
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
                if state.get_job(*dep_id).is_none() {
                    tracing::warn!(
                        dep_id = dep_id,
                        "Batch job submission failed: dependency job does not exist"
                    );
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Dependency job {} does not exist", dep_id)
                        })),
                    )
                        .into_response();
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
                )
                    .into_response();
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
        gflow::metrics::JOB_SUBMISSIONS
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
        .into_response()
}

#[axum::debug_handler]
pub(in crate::server) async fn get_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Result<Json<Job>, StatusCode> {
    let state = server_state.scheduler.read().await;
    state
        .get_job(id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[axum::debug_handler]
pub(in crate::server) async fn finish_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
    tracing::info!(job_id = id, "Finishing job");

    // Get job info before finishing (for metrics and events)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.get_job(id).map(|j| j.submitted_by.clone())
    };

    let (success, gpu_ids, memory_mb) = {
        let mut state = server_state.scheduler.write().await;
        let job_info = state
            .get_job(id)
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
            gflow::metrics::JOB_FINISHED
                .with_label_values(&[&submitted_by])
                .inc();
        }
    }

    if success {
        (StatusCode::OK, Json(())).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(())).into_response()
    }
}

#[axum::debug_handler]
pub(in crate::server) async fn get_job_log(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    // Check if job exists in memory
    if state.get_job(id).is_some() {
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
pub(in crate::server) async fn fail_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
    tracing::info!(job_id = id, "Failing job");

    // Get user and job info before failing (for metrics and events)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.get_job(id).map(|j| j.submitted_by.clone())
    };

    let (_success, gpu_ids, memory_mb) = {
        let state = server_state.scheduler.read().await;
        if let Some(job) = state.get_job(id) {
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
            gflow::metrics::JOB_FAILED
                .with_label_values(&[&submitted_by])
                .inc();
        }
    }

    if result {
        (StatusCode::OK, Json(())).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(())).into_response()
    }
}

#[axum::debug_handler]
pub(in crate::server) async fn cancel_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
    tracing::info!(job_id = id, "Cancelling job");

    // Get user and job info before cancelling (for metrics and events)
    #[cfg(feature = "metrics")]
    let user = {
        let state = server_state.scheduler.read().await;
        state.get_job(id).map(|j| j.submitted_by.clone())
    };

    let (_success, gpu_ids, memory_mb) = {
        let state = server_state.scheduler.read().await;
        if let Some(job) = state.get_job(id) {
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
            gflow::metrics::JOB_CANCELLED
                .with_label_values(&[&submitted_by])
                .inc();
        }
    }

    if result {
        (StatusCode::OK, Json(())).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(())).into_response()
    }
}

#[axum::debug_handler]
pub(in crate::server) async fn hold_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
    tracing::info!(job_id = id, "Holding job");

    let success = {
        let mut state = server_state.scheduler.write().await;
        state.hold_job(id).await
    }; // Lock released here

    if success {
        (StatusCode::OK, Json(())).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(())).into_response()
    }
}

#[axum::debug_handler]
pub(in crate::server) async fn release_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
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
        (StatusCode::OK, Json(())).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(())).into_response()
    }
}

#[axum::debug_handler]
pub(in crate::server) async fn update_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
    Json(request): Json<UpdateJobRequest>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
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
                .into_response()
        }
        Err(error) => {
            tracing::error!(job_id = id, error = %error, "Failed to update job");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": error,
                })),
            )
                .into_response()
        }
    }
}

#[axum::debug_handler]
pub(in crate::server) async fn resolve_dependency(
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
pub(in crate::server) struct ResolveDependencyQuery {
    username: String,
    shorthand: String,
}

#[axum::debug_handler]
pub(in crate::server) async fn get_health(
    State(server_state): State<ServerState>,
) -> impl IntoResponse {
    let pid = std::process::id();

    let state = server_state.scheduler.read().await;
    let state_writable = state.state_writable();
    let journal_writable = state.journal_writable();
    let mode = state.persistence_mode();
    if state_writable {
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "ok", "pid": pid })),
        );
    }

    let backup_path = state.state_backup_path().map(|p| p.display().to_string());
    let journal_path = state.journal_path().display().to_string();

    if journal_writable {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "recovery",
                "mode": mode,
                "pid": pid,
                "detail": state.state_load_error(),
                "state_backup": backup_path,
                "journal": journal_path,
                "journal_error": state.journal_error(),
            })),
        );
    }

    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "status": "read_only",
            "pid": pid,
            "detail": state.state_load_error(),
            "state_backup": backup_path,
            "journal": journal_path,
            "journal_error": state.journal_error(),
        })),
    )
}

#[derive(serde::Deserialize)]
pub(in crate::server) struct SetGpusRequest {
    allowed_indices: Option<Vec<u32>>,
}

#[axum::debug_handler]
pub(in crate::server) async fn set_allowed_gpus(
    State(server_state): State<ServerState>,
    Json(request): Json<SetGpusRequest>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
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
            )
                .into_response();
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
        .into_response()
}

#[derive(serde::Deserialize)]
pub(in crate::server) struct SetGroupMaxConcurrencyRequest {
    max_concurrent: usize,
}

#[derive(serde::Deserialize)]
pub(crate) struct UpdateJobRequest {
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
pub(in crate::server) async fn set_group_max_concurrency(
    State(server_state): State<ServerState>,
    Path(group_id): Path<String>,
    Json(request): Json<SetGroupMaxConcurrencyRequest>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
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
            .filter(|job| job.group_id.as_ref() == Some(&group_id))
            .map(|job| job.id)
            .collect();

        if job_ids.is_empty() {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("No jobs found with group_id '{}'", group_id)
                })),
            )
                .into_response();
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
        .into_response()
}
