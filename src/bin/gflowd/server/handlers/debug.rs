use super::super::state::ServerState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use gflow::core::job::JobState;
use gflow::debug;
use std::collections::HashMap;

// Debug endpoints
#[axum::debug_handler]
pub(in crate::server) async fn debug_state(
    State(server_state): State<ServerState>,
) -> impl IntoResponse {
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
pub(in crate::server) async fn debug_job(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    state
        .get_job(id)
        .cloned()
        .map(debug::DebugJobInfo::from_job)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[axum::debug_handler]
pub(in crate::server) async fn debug_metrics(
    State(server_state): State<ServerState>,
) -> impl IntoResponse {
    let state = server_state.scheduler.read().await;

    let jobs_by_state: HashMap<JobState, usize> =
        state.jobs().iter().fold(HashMap::new(), |mut acc, job| {
            *acc.entry(job.state).or_insert(0) += 1;
            acc
        });

    let jobs_by_user: HashMap<String, debug::UserJobStats> =
        state.jobs().iter().fold(HashMap::new(), |mut acc, job| {
            let stats = acc
                .entry(job.submitted_by.to_string())
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
