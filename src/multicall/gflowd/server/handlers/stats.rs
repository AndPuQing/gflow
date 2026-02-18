use super::super::state::ServerState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use gflow::core::job::JobState;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
pub(in crate::multicall::gflowd::server) struct StatsQuery {
    user: Option<String>,
    since: Option<i64>, // Unix timestamp
}

#[derive(Debug, Serialize)]
pub(in crate::multicall::gflowd::server) struct UsageStats {
    pub user: Option<String>,
    pub since: Option<u64>,

    // Job counts
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub cancelled_jobs: usize,
    pub timeout_jobs: usize,
    pub running_jobs: usize,
    pub queued_jobs: usize,

    // Timing (seconds)
    pub avg_wait_secs: Option<f64>,
    pub avg_runtime_secs: Option<f64>,
    pub total_gpu_hours: f64,

    // GPU usage
    pub jobs_with_gpus: usize,
    pub avg_gpus_per_job: f64,
    pub peak_gpu_usage: u32,

    // Rates
    pub success_rate: f64,

    // Top jobs by runtime
    pub top_jobs: Vec<TopJob>,
}

#[derive(Debug, Serialize)]
pub(in crate::multicall::gflowd::server) struct TopJob {
    pub id: u32,
    pub name: Option<String>,
    pub runtime_secs: f64,
    pub gpus: u32,
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn get_stats(
    State(server_state): State<ServerState>,
    Query(params): Query<StatsQuery>,
) -> impl IntoResponse {
    let scheduler = server_state.scheduler.read().await;
    let jobs = scheduler.jobs();

    let since_time: Option<SystemTime> = params
        .since
        .map(|secs| UNIX_EPOCH + Duration::from_secs(secs as u64));

    let filtered: Vec<_> = jobs
        .iter()
        .filter(|j| {
            // user filter
            if let Some(ref u) = params.user {
                if j.submitted_by.as_str() != u.as_str() {
                    return false;
                }
            }
            // time filter: use submitted_at
            if let Some(since) = since_time {
                if let Some(submitted) = j.submitted_at {
                    if submitted < since {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        })
        .collect();

    let total_jobs = filtered.len();
    let completed_jobs = filtered
        .iter()
        .filter(|j| j.state == JobState::Finished)
        .count();
    let failed_jobs = filtered
        .iter()
        .filter(|j| j.state == JobState::Failed)
        .count();
    let cancelled_jobs = filtered
        .iter()
        .filter(|j| j.state == JobState::Cancelled)
        .count();
    let timeout_jobs = filtered
        .iter()
        .filter(|j| j.state == JobState::Timeout)
        .count();
    let running_jobs = filtered
        .iter()
        .filter(|j| j.state == JobState::Running)
        .count();
    let queued_jobs = filtered
        .iter()
        .filter(|j| j.state == JobState::Queued)
        .count();

    // Wait times (only jobs that have started)
    let wait_times: Vec<f64> = filtered
        .iter()
        .filter_map(|j| j.wait_time().map(|d| d.as_secs_f64()))
        .collect();
    let avg_wait_secs = if wait_times.is_empty() {
        None
    } else {
        Some(wait_times.iter().sum::<f64>() / wait_times.len() as f64)
    };

    // Runtimes (only finished/failed/timeout jobs for stable averages)
    let runtimes: Vec<f64> = filtered
        .iter()
        .filter(|j| {
            matches!(
                j.state,
                JobState::Finished | JobState::Failed | JobState::Timeout
            )
        })
        .filter_map(|j| j.runtime().map(|d| d.as_secs_f64()))
        .collect();
    let avg_runtime_secs = if runtimes.is_empty() {
        None
    } else {
        Some(runtimes.iter().sum::<f64>() / runtimes.len() as f64)
    };

    // GPU hours: sum of (gpus * runtime_hours) for all jobs with runtime
    let total_gpu_hours: f64 = filtered
        .iter()
        .filter_map(|j| {
            j.runtime()
                .map(|rt| j.gpus as f64 * rt.as_secs_f64() / 3600.0)
        })
        .sum();

    let jobs_with_gpus = filtered.iter().filter(|j| j.gpus > 0).count();
    let avg_gpus_per_job = if total_jobs == 0 {
        0.0
    } else {
        filtered.iter().map(|j| j.gpus as f64).sum::<f64>() / total_jobs as f64
    };
    let peak_gpu_usage = filtered.iter().map(|j| j.gpus).max().unwrap_or(0);

    let terminal_jobs = completed_jobs + failed_jobs + cancelled_jobs + timeout_jobs;
    let success_rate = if terminal_jobs == 0 {
        0.0
    } else {
        completed_jobs as f64 / terminal_jobs as f64 * 100.0
    };

    // Top 5 jobs by runtime
    let mut jobs_with_runtime: Vec<(&gflow::core::job::Job, std::time::Duration)> = filtered
        .iter()
        .filter_map(|j| j.runtime().map(|rt| (*j, rt)))
        .collect();
    jobs_with_runtime.sort_by_key(|b| std::cmp::Reverse(b.1));
    let top_jobs: Vec<TopJob> = jobs_with_runtime
        .iter()
        .take(5)
        .map(|(j, rt)| TopJob {
            id: j.id,
            name: j
                .run_name
                .as_ref()
                .map(|s: &compact_str::CompactString| s.to_string()),
            runtime_secs: rt.as_secs_f64(),
            gpus: j.gpus,
        })
        .collect();

    let stats = UsageStats {
        user: params.user,
        since: params.since.map(|s| s as u64),
        total_jobs,
        completed_jobs,
        failed_jobs,
        cancelled_jobs,
        timeout_jobs,
        running_jobs,
        queued_jobs,
        avg_wait_secs,
        avg_runtime_secs,
        total_gpu_hours,
        jobs_with_gpus,
        avg_gpus_per_job,
        peak_gpu_usage,
        success_rate,
        top_jobs,
    };

    (StatusCode::OK, Json(stats))
}
