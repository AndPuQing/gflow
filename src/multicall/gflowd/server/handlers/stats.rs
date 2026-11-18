use super::super::state::ServerState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use gflow::client::{TopJob, UsageStats};
use gflow::core::job::{JobRuntime, JobSpec, JobState};
use serde::Deserialize;
use std::time::{Duration, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
pub(in crate::multicall::gflowd::server) struct StatsQuery {
    user: Option<String>,
    since: Option<i64>, // Unix timestamp
}

#[axum::debug_handler]
pub(in crate::multicall::gflowd::server) async fn get_stats(
    State(server_state): State<ServerState>,
    Query(params): Query<StatsQuery>,
) -> impl IntoResponse {
    let scheduler = server_state.scheduler.read().await;
    let stats = compute_stats(scheduler.job_specs(), scheduler.job_runtimes(), params);
    (StatusCode::OK, Json(stats))
}

fn compute_stats(specs: &[JobSpec], runtimes: &[JobRuntime], params: StatsQuery) -> UsageStats {
    const TOP_JOB_COUNT: usize = 5;

    let since = params.since.map(|secs| secs.max(0) as u64);
    let cutoff = since.map(Duration::from_secs);
    let mut stats = UsageStats {
        user: params.user,
        since,
        ..UsageStats::default()
    };

    let mut wait_total = 0.0;
    let mut wait_count = 0usize;
    let mut runtime_total = 0.0;
    let mut runtime_count = 0usize;
    let mut gpu_total = 0.0;
    let mut top_jobs: Vec<(&JobSpec, &JobRuntime, Duration)> = Vec::with_capacity(TOP_JOB_COUNT);

    for (spec, rt) in specs.iter().zip(runtimes) {
        if stats
            .user
            .as_deref()
            .is_some_and(|user| spec.submitted_by != user)
            || cutoff.is_some_and(|cutoff| {
                spec.submitted_at
                    .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
                    .is_none_or(|submitted| submitted < cutoff)
            })
        {
            continue;
        }

        stats.total_jobs += 1;
        match rt.state {
            JobState::Finished => stats.completed_jobs += 1,
            JobState::Failed => stats.failed_jobs += 1,
            JobState::Cancelled => stats.cancelled_jobs += 1,
            JobState::Timeout => stats.timeout_jobs += 1,
            JobState::Running => stats.running_jobs += 1,
            JobState::Queued => stats.queued_jobs += 1,
            JobState::Hold => {}
        }

        stats.jobs_with_gpus += usize::from(rt.gpus > 0);
        gpu_total += f64::from(rt.gpus);
        stats.peak_gpu_usage = stats.peak_gpu_usage.max(rt.gpus);

        if let Some(wait) = spec
            .submitted_at
            .zip(rt.started_at)
            .and_then(|(submitted, started)| started.duration_since(submitted).ok())
        {
            wait_total += wait.as_secs_f64();
            wait_count += 1;
        }

        if let Some(runtime) = rt.runtime() {
            let runtime_secs = runtime.as_secs_f64();
            stats.total_gpu_hours += f64::from(rt.gpus) * runtime_secs / 3600.0;
            if matches!(
                rt.state,
                JobState::Finished | JobState::Failed | JobState::Timeout
            ) {
                runtime_total += runtime_secs;
                runtime_count += 1;
            }

            // Equal runtimes keep their original job ID order.
            let position = top_jobs.partition_point(|&(_, _, duration)| duration >= runtime);
            if position < TOP_JOB_COUNT {
                if top_jobs.len() == TOP_JOB_COUNT {
                    top_jobs.pop();
                }
                top_jobs.insert(position, (spec, rt, runtime));
            }
        }
    }

    if wait_count > 0 {
        stats.avg_wait_secs = Some(wait_total / wait_count as f64);
    }
    if runtime_count > 0 {
        stats.avg_runtime_secs = Some(runtime_total / runtime_count as f64);
    }
    if stats.total_jobs > 0 {
        stats.avg_gpus_per_job = gpu_total / stats.total_jobs as f64;
    }

    let terminal_jobs =
        stats.completed_jobs + stats.failed_jobs + stats.cancelled_jobs + stats.timeout_jobs;
    if terminal_jobs > 0 {
        stats.success_rate = stats.completed_jobs as f64 / terminal_jobs as f64 * 100.0;
    }
    stats.top_jobs = top_jobs
        .into_iter()
        .map(|(spec, rt, duration)| TopJob {
            id: rt.id,
            name: spec.run_name.as_ref().map(ToString::to_string),
            runtime_secs: duration.as_secs_f64(),
            gpus: rt.gpus,
        })
        .collect();

    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_filter_by_user_and_time_and_keep_equal_runtime_jobs_in_id_order() {
        let cutoff = UNIX_EPOCH + Duration::from_secs(100);
        let records = [
            ("alice", Some(cutoff), JobState::Finished, 10),
            ("alice", Some(cutoff), JobState::Failed, 10),
            ("alice", Some(cutoff), JobState::Cancelled, 30),
            ("alice", Some(cutoff), JobState::Timeout, 10),
            ("alice", Some(cutoff), JobState::Finished, 10),
            ("alice", Some(cutoff), JobState::Finished, 10),
            ("bob", Some(cutoff), JobState::Finished, 100),
            ("alice", Some(UNIX_EPOCH), JobState::Finished, 100),
            ("alice", None, JobState::Finished, 100),
        ];
        let (specs, runtimes): (Vec<_>, Vec<_>) = records
            .into_iter()
            .enumerate()
            .map(|(index, (user, submitted_at, state, seconds))| {
                let started_at = cutoff + Duration::from_secs(2);
                (
                    JobSpec {
                        submitted_by: user.into(),
                        submitted_at,
                        ..JobSpec::default()
                    },
                    JobRuntime {
                        id: index as u32 + 1,
                        gpus: 2,
                        state,
                        started_at: Some(started_at),
                        finished_at: Some(started_at + Duration::from_secs(seconds)),
                        ..JobRuntime::default()
                    },
                )
            })
            .unzip();
        let stats = compute_stats(
            &specs,
            &runtimes,
            StatsQuery {
                user: Some("alice".into()),
                since: Some(100),
            },
        );

        assert_eq!(stats.total_jobs, 6);
        assert_eq!(stats.completed_jobs, 3);
        assert_eq!(stats.failed_jobs, 1);
        assert_eq!(stats.cancelled_jobs, 1);
        assert_eq!(stats.timeout_jobs, 1);
        assert_eq!(stats.avg_wait_secs, Some(2.0));
        assert_eq!(stats.avg_runtime_secs, Some(10.0));
        assert!((stats.total_gpu_hours - 160.0 / 3600.0).abs() < 1e-12);
        assert_eq!(stats.success_rate, 50.0);
        assert_eq!(
            stats.top_jobs.iter().map(|job| job.id).collect::<Vec<_>>(),
            [3, 1, 2, 4, 5]
        );
    }
}
