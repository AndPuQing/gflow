use gflow::core::job::Job;
use serde_json::{json, Value};

use super::helpers::system_time_to_unix_secs;
use super::schemas::{
    ListJobsDetailInput, ListJobsOrderInput, ListJobsRequest, DEFAULT_MCP_LIST_JOBS_LIMIT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResolvedListJobsPage {
    pub limit: usize,
    pub offset: usize,
    pub order: ListJobsOrderInput,
    pub detail: ListJobsDetailInput,
    pub query_limit: usize,
}

pub(super) fn resolve_list_jobs_page(params: &ListJobsRequest) -> ResolvedListJobsPage {
    let limit = params.limit.unwrap_or(DEFAULT_MCP_LIST_JOBS_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let order = params.order.unwrap_or(ListJobsOrderInput::Desc);
    let detail = params.detail.unwrap_or(ListJobsDetailInput::Summary);

    ResolvedListJobsPage {
        limit,
        offset,
        order,
        detail,
        query_limit: limit.saturating_add(1),
    }
}

pub(super) fn serialize_list_job(job: Job, detail: ListJobsDetailInput) -> Value {
    match detail {
        ListJobsDetailInput::Summary => json!({
            "id": job.id,
            "name": job.run_name,
            "state": job.state,
            "reason": job.reason.as_deref().map(ToString::to_string),
            "gpus": job.gpus,
            "gpu_ids": job.gpu_ids,
            "user": job.submitted_by,
            "project": job.project,
            "submitted": job.submitted_at.and_then(system_time_to_unix_secs),
            "started": job.started_at.and_then(system_time_to_unix_secs),
            "finished": job.finished_at.and_then(system_time_to_unix_secs),
        }),
        ListJobsDetailInput::Full => serde_json::to_value(job).unwrap_or_else(|err| {
            json!({
                "error": format!("Failed to serialize job: {}", err),
            })
        }),
    }
}
