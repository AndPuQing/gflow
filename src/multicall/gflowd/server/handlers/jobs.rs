use super::super::state::{reject_if_read_only, ServerState};
use crate::multicall::gflowd::events::SchedulerEvent;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use gflow::client::{ListJobsQuery, LIST_JOBS_TOTAL_HEADER};
use gflow::core::job::{Job, JobRuntime, JobSpec, JobState};
use std::borrow::Cow;
use std::collections::HashMap;

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn info(
    State(server_state): State<ServerState>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;
    let info = state.info();
    (StatusCode::OK, Json(info))
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn daemon_status(
    State(server_state): State<ServerState>,
) -> impl IntoResponse {
    use gflow::core::info::DaemonStatus;

    let state = server_state.scheduler.read().await;
    let info = state.info();
    let gpu_total = info.gpus.len();
    let gpu_available = info.gpus.iter().filter(|g| g.available).count();
    let status = DaemonStatus {
        version: gflow::build_info::version()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string(),
        pid: std::process::id(),
        uptime_secs: server_state.started_at.elapsed().as_secs(),
        executor: info.executor,
        gpu_total,
        gpu_available,
    };
    (StatusCode::OK, Json(status))
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn list_ignored_gpu_processes(
    State(server_state): State<ServerState>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;
    let processes = state.list_ignored_gpu_processes();
    (StatusCode::OK, Json(processes))
}

fn merge_job_ids<'a>(mut indexes: impl Iterator<Item = &'a [u32]>) -> Cow<'a, [u32]> {
    let first = indexes.next().unwrap_or_default();
    let Some(second) = indexes.next() else {
        return Cow::Borrowed(first);
    };

    let mut ids = first.to_vec();
    ids.extend_from_slice(second);
    ids.extend(indexes.flatten().copied());
    ids.sort_unstable();
    ids.dedup();
    Cow::Owned(ids)
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn list_jobs(
    State(server_state): State<ServerState>,
    Query(params): Query<ListJobsQuery>,
) -> Result<Response, StatusCode> {
    let states: Vec<JobState> = params
        .state
        .iter()
        .flat_map(|states| states.split(','))
        .filter_map(|state| state.trim().parse().ok())
        .collect();
    let users: Vec<&str> = params
        .user
        .iter()
        .flat_map(|users| users.split(','))
        .map(str::trim)
        .collect();
    let time_filter = params.created_after.and_then(|secs| {
        use std::time::{Duration, UNIX_EPOCH};
        UNIX_EPOCH.checked_add(Duration::from_secs(secs.max(0) as u64))
    });

    let state = server_state.scheduler.read().await;
    let specs = state.job_specs();
    let runtimes = state.job_runtimes();
    // Use the smaller index when both filters are present.
    let user_ids = users.iter().filter_map(|user| state.job_ids_by_user(user));
    let state_ids = states
        .iter()
        .filter_map(|&state_name| state.job_ids_by_state(state_name));
    let candidate_ids = match (users.is_empty(), states.is_empty()) {
        (true, true) => None,
        (false, true) => Some(merge_job_ids(user_ids)),
        (true, false) => Some(merge_job_ids(state_ids)),
        (false, false) => {
            let user_count = user_ids.clone().map(|ids| ids.len()).sum::<usize>();
            let state_count = state_ids.clone().map(|ids| ids.len()).sum::<usize>();
            Some(if user_count <= state_count {
                merge_job_ids(user_ids)
            } else {
                merge_job_ids(state_ids)
            })
        }
    };

    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(usize::MAX);
    let descending = params
        .order
        .as_deref()
        .is_some_and(|order| order.eq_ignore_ascii_case("desc"));
    let mut matched = 0;
    let mut jobs = Vec::new();
    let mut collect_job = |(spec, rt): (&JobSpec, &JobRuntime)| {
        if (!states.is_empty() && !states.contains(&rt.state))
            || (!users.is_empty() && !users.contains(&spec.submitted_by.as_str()))
            || time_filter.is_some_and(|cutoff| spec.submitted_at.is_none_or(|ts| ts < cutoff))
        {
            return false;
        }

        if matched >= offset && jobs.len() < limit {
            jobs.push(Job::from_parts(spec.clone(), rt.clone()));
        }
        matched += 1;
        !params.include_total && jobs.len() >= limit
    };

    if let Some(ids) = candidate_ids {
        let visit_id = |&id: &u32| {
            let Some(index) = id.checked_sub(1).map(|index| index as usize) else {
                return false;
            };
            specs
                .get(index)
                .zip(runtimes.get(index))
                .is_some_and(&mut collect_job)
        };
        if descending {
            ids.iter().rev().any(visit_id);
        } else {
            ids.iter().any(visit_id);
        }
    } else {
        let mut records = specs.iter().zip(runtimes);
        if descending {
            records.rev().any(collect_job);
        } else {
            records.any(collect_job);
        }
    }

    let executor = state.executor();
    drop(state);
    // Liveness is a display hint. Probe after snapshotting the page and releasing the lock.
    if jobs.iter().any(|job| job.state == JobState::Running) {
        jobs = tokio::task::spawn_blocking(move || {
            executor.annotate_liveness(&mut jobs);
            jobs
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "Job liveness probe failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    let mut response = Json(jobs).into_response();
    if params.include_total {
        response
            .headers_mut()
            .insert(LIST_JOBS_TOTAL_HEADER, HeaderValue::from(matched));
    }
    Ok(response)
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn create_job(
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

        let (job_id, run_name, _job_clone) = match state.submit_job(input).await {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(%error, "Job submission failed: project policy validation");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": error.to_string()
                    })),
                )
                    .into_response();
            }
        };
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
pub(in crate::multicall::gflowd::server) async fn create_jobs_batch(
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

        match state.submit_jobs(input).await {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(%error, "Batch job submission failed: project policy validation");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": error.to_string()
                    })),
                )
                    .into_response();
            }
        }
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
pub(in crate::multicall::gflowd::server) async fn get_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Result<Json<Job>, StatusCode> {
    let state = server_state.scheduler.read().await;
    let Some(mut job) = state.get_job(id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let executor = state.executor();
    drop(state);
    executor.annotate_liveness(std::slice::from_mut(&mut job));
    Ok(Json(job))
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn finish_job(
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
pub(in crate::multicall::gflowd::server) async fn get_job_log(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    // Check if job exists in memory
    if state.get_job(id).is_some() {
        match gflow::paths::get_log_file_path(id) {
            Ok(path) => match gflow::paths::get_data_dir() {
                Ok(data_dir) => {
                    let log_dir = data_dir.join("logs");
                    if !path.starts_with(&log_dir) {
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(None))
                    } else if path.exists() {
                        (StatusCode::OK, Json(Some(path)))
                    } else {
                        (StatusCode::NOT_FOUND, Json(None))
                    }
                }
                Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(None)),
            },
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(None)),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

#[derive(serde::Deserialize)]
pub(in crate::multicall::gflowd::server) struct JobLogContentQuery {
    /// Maximum number of trailing lines to return.
    tail: Option<usize>,
    /// Maximum number of trailing bytes read from the file.
    max_bytes: Option<usize>,
}

#[derive(serde::Serialize)]
pub(in crate::multicall::gflowd::server) struct JobLogContent {
    job_id: u32,
    path: String,
    /// Total size of the log file in bytes.
    size: u64,
    /// Whether the content was truncated (by bytes or by lines).
    truncated: bool,
    content: String,
}

const DEFAULT_LOG_MAX_BYTES: usize = 256 * 1024;
const MAX_LOG_MAX_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_LOG_TAIL_LINES: usize = 1000;
const MAX_LOG_TAIL_LINES: usize = 20_000;

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn get_job_log_content(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
    axum::extract::Query(params): axum::extract::Query<JobLogContentQuery>,
) -> Response {
    let state = server_state.scheduler.read().await;

    // Check if job exists in memory
    if state.get_job(id).is_none() {
        return (StatusCode::NOT_FOUND, Json(None::<()>)).into_response();
    }

    let path = match gflow::paths::get_log_file_path(id) {
        Ok(path) => path,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(None::<()>)).into_response(),
    };

    // Never serve files outside the log directory
    let data_dir = match gflow::paths::get_data_dir() {
        Ok(data_dir) => data_dir,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(None::<()>)).into_response(),
    };
    if !path.starts_with(data_dir.join("logs")) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(None::<()>)).into_response();
    }
    if !path.exists() {
        return (StatusCode::NOT_FOUND, Json(None::<()>)).into_response();
    }

    let max_bytes = params
        .max_bytes
        .unwrap_or(DEFAULT_LOG_MAX_BYTES)
        .clamp(1, MAX_LOG_MAX_BYTES);
    let tail_lines = params
        .tail
        .unwrap_or(DEFAULT_LOG_TAIL_LINES)
        .clamp(1, MAX_LOG_TAIL_LINES);

    match read_log_tail(&path, max_bytes, tail_lines).await {
        Ok((size, view)) => (
            StatusCode::OK,
            Json(Some(JobLogContent {
                job_id: id,
                path: path.display().to_string(),
                size,
                truncated: view.truncated,
                content: view.content,
            })),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!(job_id = id, %error, "Failed to read job log");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(None::<()>)).into_response()
        }
    }
}

/// A tail view of a log file.
#[derive(Debug, PartialEq)]
struct LogView {
    content: String,
    truncated: bool,
}

/// Read up to the last `max_bytes` of the file and keep the last `tail_lines`
/// lines of that slice.
async fn read_log_tail(
    path: &std::path::Path,
    max_bytes: usize,
    tail_lines: usize,
) -> anyhow::Result<(u64, LogView)> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();

    let read_bytes = (size as usize).min(max_bytes);
    let started_mid_file = (size as usize) > read_bytes;
    if started_mid_file {
        file.seek(std::io::SeekFrom::End(-(read_bytes as i64)))
            .await?;
    }

    let mut buffer = Vec::with_capacity(read_bytes);
    file.read_to_end(&mut buffer).await?;

    let raw = String::from_utf8_lossy(&buffer);
    let view = extract_log_view(&raw, started_mid_file, tail_lines);
    Ok((size, view))
}

/// Reduce a (possibly partial) log slice to its trailing lines.
///
/// When `started_mid_file` is set, the first line is likely partial and is
/// dropped so callers never see a cut-off line.
fn extract_log_view(raw: &str, started_mid_file: bool, tail_lines: usize) -> LogView {
    let text = if started_mid_file {
        raw.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
    } else {
        raw
    };

    let total_lines = text.lines().count();
    if total_lines <= tail_lines {
        return LogView {
            content: text.to_string(),
            truncated: started_mid_file,
        };
    }

    let content = text
        .lines()
        .skip(total_lines - tail_lines)
        .collect::<Vec<_>>()
        .join("\n");
    LogView {
        content,
        truncated: true,
    }
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn fail_job(
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
        state.explicit_fail_job(id).await
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

        #[cfg(feature = "metrics")]
        if let Some(submitted_by) = user {
            gflow::metrics::JOB_FAILED
                .with_label_values(&[&submitted_by])
                .inc();
        }

        (StatusCode::OK, Json(())).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(())).into_response()
    }
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn cancel_job(
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
pub(in crate::multicall::gflowd::server) async fn hold_job(
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

/// Optional query parameters for the release endpoint.
#[derive(Debug, serde::Deserialize)]
pub(in crate::multicall::gflowd::server) struct ReleaseQuery {
    /// Unix seconds (epoch) at which the job should start. When present the
    /// release is deferred: the job is queued now but is not runnable before
    /// this time (delayed release, `gjob release --at <time>`).
    pub at: Option<i64>,
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn release_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
    Query(query): Query<ReleaseQuery>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }
    tracing::info!(job_id = id, at = ?query.at, "Releasing job");

    let success = {
        let mut state = server_state.scheduler.write().await;
        match query.at {
            Some(unix_secs) if unix_secs >= 0 => {
                let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs as u64);
                state.release_job_at(id, at).await
            }
            Some(_) => {
                tracing::error!(job_id = id, "Invalid release time (negative unix seconds)");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Invalid release time: 'at' must be a non-negative unix timestamp"
                    })),
                )
                    .into_response();
            }
            None => state.release_job(id).await,
        }
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
pub(in crate::multicall::gflowd::server) async fn update_job(
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

            // Trigger re-scheduling in case updated fields affect scheduling decisions
            server_state
                .event_bus
                .publish(SchedulerEvent::JobUpdated { job_id: id });

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
pub(in crate::multicall::gflowd::server) async fn resolve_dependency(
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
pub(in crate::multicall::gflowd::server) struct ResolveDependencyQuery {
    username: String,
    shorthand: String,
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn get_health(
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
pub(in crate::multicall::gflowd::server) struct SetGpusRequest {
    allowed_indices: Option<Vec<u32>>,
}

#[derive(serde::Deserialize)]
pub(in crate::multicall::gflowd::server) struct GpuProcessOverrideRequest {
    gpu_index: u32,
    pid: u32,
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn set_allowed_gpus(
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

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn ignore_gpu_process(
    State(server_state): State<ServerState>,
    Json(request): Json<GpuProcessOverrideRequest>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }

    let mut state = server_state.scheduler.write().await;
    let available_before = state.gpu_available(request.gpu_index);
    let result = state.ignore_gpu_process(request.gpu_index, request.pid);
    let available_after = state.gpu_available(request.gpu_index);
    drop(state);

    match result {
        Ok(inserted) => {
            tracing::warn!(
                gpu_index = request.gpu_index,
                pid = request.pid,
                inserted,
                "Manual GPU process ignore override updated"
            );
            if let (Some(before), Some(after)) = (available_before, available_after) {
                if before != after {
                    server_state
                        .event_bus
                        .publish(SchedulerEvent::ManualGpuOverrideChanged {
                            gpu_index: request.gpu_index,
                            available: after,
                        });
                }
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "gpu_index": request.gpu_index,
                    "pid": request.pid,
                    "inserted": inserted,
                    "available": available_after,
                })),
            )
                .into_response()
        }
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": error.to_string()
            })),
        )
            .into_response(),
    }
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn unignore_gpu_process(
    State(server_state): State<ServerState>,
    Json(request): Json<GpuProcessOverrideRequest>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }

    let mut state = server_state.scheduler.write().await;
    let available_before = state.gpu_available(request.gpu_index);
    let removed = state.unignore_gpu_process(request.gpu_index, request.pid);
    let available_after = state.gpu_available(request.gpu_index);
    drop(state);

    if !removed {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!(
                    "Ignore override for PID {} on GPU {} was not found",
                    request.pid, request.gpu_index
                )
            })),
        )
            .into_response();
    }

    tracing::info!(
        gpu_index = request.gpu_index,
        pid = request.pid,
        "Manual GPU process ignore override removed"
    );
    if let (Some(before), Some(after)) = (available_before, available_after) {
        if before != after {
            server_state
                .event_bus
                .publish(SchedulerEvent::ManualGpuOverrideChanged {
                    gpu_index: request.gpu_index,
                    available: after,
                });
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "gpu_index": request.gpu_index,
            "pid": request.pid,
            "removed": true,
            "available": available_after,
        })),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
pub(in crate::multicall::gflowd::server) struct SetGroupMaxConcurrencyRequest {
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
    pub gpu_memory_limit_mb: Option<Option<u64>>,
    pub depends_on_ids: Option<Vec<u32>>,
    pub dependency_mode: Option<Option<gflow::core::job::DependencyMode>>,
    pub auto_cancel_on_dependency_failure: Option<bool>,
    pub max_concurrent: Option<Option<usize>>,
    pub max_retries: Option<Option<u32>>,
    pub notifications: Option<gflow::core::job::JobNotifications>,
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn set_group_max_concurrency(
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

    // Parse group_id string to UUID
    let group_uuid = match uuid::Uuid::parse_str(&group_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid UUID format: '{}'", group_id)
                })),
            )
                .into_response();
        }
    };

    let updated_jobs = {
        let mut state = server_state.scheduler.write().await;

        // Find all jobs in this group and collect their IDs
        let job_ids: Vec<u32> = state
            .job_runtimes()
            .iter()
            .filter(|rt| rt.group_id.as_ref() == Some(&group_uuid))
            .map(|rt| rt.id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_log_view_keeps_full_text_when_under_limit() {
        let view = extract_log_view("line1\nline2\n", false, 10);
        assert_eq!(view.content, "line1\nline2\n");
        assert!(!view.truncated);
    }

    #[test]
    fn extract_log_view_keeps_only_trailing_lines() {
        let raw = "a\nb\nc\nd\n";
        let view = extract_log_view(raw, false, 2);
        assert_eq!(view.content, "c\nd");
        assert!(view.truncated);
    }

    #[test]
    fn extract_log_view_drops_partial_first_line_when_started_mid_file() {
        let raw = "partial-line\nc\nd\n";
        let view = extract_log_view(raw, true, 10);
        assert_eq!(view.content, "c\nd\n");
        assert!(view.truncated);
    }

    #[test]
    fn extract_log_view_marks_truncation_even_when_lines_fit() {
        let raw = "partial\nc\nd\n";
        let view = extract_log_view(raw, true, 100);
        assert!(view.truncated);
    }

    #[tokio::test]
    async fn read_log_tail_reads_entire_small_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("1.log");
        std::fs::write(&path, "hello\nworld\n").unwrap();

        let (size, view) = read_log_tail(&path, 1024, 100).await.unwrap();
        assert_eq!(size, 12);
        assert_eq!(view.content, "hello\nworld\n");
        assert!(!view.truncated);
    }

    #[tokio::test]
    async fn read_log_tail_caps_bytes_and_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("2.log");
        let lines: Vec<String> = (0..100).map(|i| format!("line-{i:03}")).collect();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        // Byte cap cuts the file; line cap keeps only the last 5 lines.
        let (size, view) = read_log_tail(&path, 64, 5).await.unwrap();
        assert!(size > 64);
        assert!(view.truncated);
        assert_eq!(
            view.content,
            "line-095\nline-096\nline-097\nline-098\nline-099"
        );
    }
}
