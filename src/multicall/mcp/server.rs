use anyhow::Result;
use clap_verbosity_flag::Verbosity;
use compact_str::CompactString;
use gflow::client::{UpdateJobRequest, UpdateJobResponse};
use gflow::core::job::{
    DependencyMode, GpuSharingMode, Job, JobBuilder, JobNotifications, JobState,
};
use gflow::core::reservation::ReservationStatus;
use gflow::utils::{generate_param_combinations, parse_param_spec};
use gflow::Client;
use lettre::message::Mailbox;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::stdio,
    ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_MCP_LIST_JOBS_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
pub enum DependencyModeInput {
    All,
    Any,
}

impl From<DependencyModeInput> for DependencyMode {
    fn from(value: DependencyModeInput) -> Self {
        match value {
            DependencyModeInput::All => DependencyMode::All,
            DependencyModeInput::Any => DependencyMode::Any,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ListJobsOrderInput {
    Asc,
    Desc,
}

impl ListJobsOrderInput {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ListJobsDetailInput {
    Summary,
    Full,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListJobsRequest {
    /// Comma-separated job states, for example `Running,Finished`.
    pub state: Option<String>,
    /// Filter by submitting user.
    pub user: Option<String>,
    /// Defaults to 50 when omitted.
    pub limit: Option<usize>,
    /// Zero-based offset into the filtered result set.
    pub offset: Option<usize>,
    /// Unix timestamp in seconds.
    pub created_after: Option<i64>,
    /// Defaults to `desc` so recent jobs are returned first.
    pub order: Option<ListJobsOrderInput>,
    /// Defaults to `summary` to keep MCP responses compact. Use `full` for the entire job object.
    pub detail: Option<ListJobsDetailInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JobIdRequest {
    pub job_id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetJobLogRequest {
    pub job_id: u32,
    /// Return only the first N lines.
    pub first_lines: Option<usize>,
    /// Return only the last N lines.
    #[serde(alias = "tail_lines")]
    pub last_lines: Option<usize>,
    /// Truncate to the last N bytes.
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetStatsRequest {
    pub user: Option<String>,
    /// Unix timestamp in seconds.
    pub since: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TriageJobRequest {
    pub job_id: u32,
    /// Defaults to the last 80 lines.
    #[serde(alias = "tail_lines")]
    pub last_lines: Option<usize>,
    /// Defaults to the last 20000 bytes after slicing.
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SubmitJobRequest {
    pub command: Option<String>,
    pub script: Option<String>,
    pub gpus: Option<u32>,
    pub conda_env: Option<String>,
    pub run_dir: Option<PathBuf>,
    pub priority: Option<u8>,
    pub depends_on: Option<u32>,
    pub depends_on_ids: Option<Vec<u32>>,
    pub dependency_mode: Option<DependencyModeInput>,
    pub auto_cancel_on_dependency_failure: Option<bool>,
    pub shared: Option<bool>,
    pub gpu_memory_limit_mb: Option<u64>,
    pub time_limit_secs: Option<u64>,
    pub memory_limit_mb: Option<u64>,
    pub submitted_by: Option<String>,
    /// CLI-style parameter sweep definitions such as `lr=0.001,0.01`.
    /// Each input job can expand into multiple jobs via cartesian product.
    pub param: Option<Vec<String>>,
    /// Concrete parameter values used for `{name}` substitution.
    pub parameters: Option<HashMap<String, String>>,
    pub run_name: Option<String>,
    pub project: Option<String>,
    pub max_concurrent: Option<usize>,
    pub max_retries: Option<u32>,
    pub auto_close_tmux: Option<bool>,
    /// Additional email recipient for this job's notifications.
    pub notify_email: Option<Vec<String>>,
    /// Event names for this job's notifications (defaults to terminal events when omitted).
    pub notify_on: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubmitJobsRequest {
    pub jobs: Vec<SubmitJobRequest>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateJobToolRequest {
    pub job_id: u32,
    pub command: Option<String>,
    pub script: Option<String>,
    pub gpus: Option<u32>,
    pub conda_env: Option<String>,
    pub clear_conda_env: Option<bool>,
    pub priority: Option<u8>,
    pub parameters: Option<HashMap<String, String>>,
    pub time_limit_secs: Option<u64>,
    pub clear_time_limit: Option<bool>,
    pub memory_limit_mb: Option<u64>,
    pub clear_memory_limit: Option<bool>,
    pub gpu_memory_limit_mb: Option<u64>,
    pub clear_gpu_memory_limit: Option<bool>,
    pub depends_on_ids: Option<Vec<u32>>,
    pub dependency_mode: Option<DependencyModeInput>,
    pub clear_dependency_mode: Option<bool>,
    pub auto_cancel_on_dependency_failure: Option<bool>,
    pub max_concurrent: Option<usize>,
    pub clear_max_concurrent: Option<bool>,
    pub max_retries: Option<u32>,
    pub clear_max_retries: Option<bool>,
    /// Replace this job's email notification recipients. Use an empty list to clear notifications.
    pub notify_email: Option<Vec<String>>,
    /// Replace this job's notification events. Requires `notify_email` in the same request.
    pub notify_on: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RedoJobRequest {
    pub job_id: u32,
    pub gpus: Option<u32>,
    pub priority: Option<u8>,
    pub depends_on: Option<u32>,
    pub time_limit_secs: Option<u64>,
    pub memory_limit_mb: Option<u64>,
    pub gpu_memory_limit_mb: Option<u64>,
    pub conda_env: Option<String>,
    pub clear_deps: Option<bool>,
    pub cascade: Option<bool>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct ArbitraryObjectSchema {
    #[schemars(flatten)]
    pub fields: HashMap<String, Value>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct HealthOutput {
    pub status: u16,
    pub ok: bool,
    pub pid: Option<u32>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct GpuInfoOutput {
    pub uuid: String,
    pub index: u32,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct SchedulerInfoOutput {
    pub gpus: Vec<GpuInfoOutput>,
    pub allowed_gpu_indices: Option<Vec<u32>>,
    pub gpu_allocation_strategy: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct ListJobsOutput {
    pub jobs: Vec<Value>,
    pub count: usize,
    pub detail: ListJobsDetailInput,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct ListReservationsOutput {
    pub reservations: Vec<Value>,
    pub count: usize,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct PreviewSubmitJobOutput {
    pub dry_run: bool,
    pub valid: bool,
    pub input_count: usize,
    pub expanded_count: usize,
    pub jobs: Vec<PreviewSubmitJobResultOutput>,
    pub warnings: Vec<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct PreviewSubmitJobResultOutput {
    pub input_index: usize,
    pub expanded_index: usize,
    pub ok: bool,
    pub job: Option<Value>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct PreviewUpdateJobOutput {
    pub dry_run: bool,
    pub ok: bool,
    pub job_id: u32,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub updated_fields: Vec<String>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct TriageJobOutput {
    pub job_id: u32,
    pub state: String,
    pub reason: Option<String>,
    pub requested_gpus: u32,
    pub gpu_ids: Option<Vec<u32>>,
    pub runtime_secs: Option<f64>,
    pub wait_secs: Option<f64>,
    pub exit_status: Option<i32>,
    pub exit_status_note: String,
    pub log_path: Option<String>,
    pub log_excerpt: Option<String>,
    pub retry_hints: Vec<String>,
    pub job: Value,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct QueuePressureOutput {
    pub generated_at: u64,
    pub total_gpus: usize,
    pub available_gpus: Vec<u32>,
    pub unavailable_gpus: Vec<GpuAvailabilityOutput>,
    pub running_jobs: usize,
    pub queued_jobs: usize,
    pub held_jobs: usize,
    pub queued_requested_gpus: u32,
    pub running_allocated_gpus: u32,
    pub blocked_reasons: BTreeMap<String, usize>,
    pub users: Vec<QueuePressureGroupOutput>,
    pub projects: Vec<QueuePressureGroupOutput>,
    pub reservations_total: usize,
    pub reservations_active: usize,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct GpuAvailabilityOutput {
    pub index: u32,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct QueuePressureGroupOutput {
    pub name: String,
    pub queued: usize,
    pub running: usize,
    pub requested_gpus: u32,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct JobLogOutput {
    pub job_id: u32,
    pub log_path: String,
    pub text: String,
    pub program_output: Option<String>,
    pub full_text: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct TopJobOutput {
    pub id: u32,
    pub name: Option<String>,
    pub runtime_secs: f64,
    pub gpus: u32,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct UsageStatsOutput {
    pub user: Option<String>,
    pub since: Option<u64>,
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub cancelled_jobs: usize,
    pub timeout_jobs: usize,
    pub running_jobs: usize,
    pub queued_jobs: usize,
    pub avg_wait_secs: Option<f64>,
    pub avg_runtime_secs: Option<f64>,
    pub total_gpu_hours: f64,
    pub jobs_with_gpus: usize,
    pub avg_gpus_per_job: f64,
    pub peak_gpu_usage: u32,
    pub success_rate: f64,
    pub top_jobs: Vec<TopJobOutput>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct JobActionOutput {
    pub job_id: u32,
    pub cancelled: Option<bool>,
    pub held: Option<bool>,
    pub released: Option<bool>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct SubmitJobResultOutput {
    pub index: usize,
    pub ok: bool,
    pub job_id: Option<u32>,
    pub run_name: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct SubmitJobsOutput {
    pub results: Vec<SubmitJobResultOutput>,
    pub submitted: usize,
    pub failed: usize,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct UpdateJobOutputSchema {
    pub job: ArbitraryObjectSchema,
    pub updated_fields: Vec<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct RedoCascadeJobOutput {
    pub original_job_id: u32,
    pub new_job_id: u32,
    pub run_name: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct RedoJobOutput {
    pub original_job_id: u32,
    pub new_job_id: u32,
    pub run_name: String,
    pub cascaded_jobs: Vec<RedoCascadeJobOutput>,
    pub cascaded_count: usize,
}

#[derive(Clone)]
struct GflowMcpServer {
    config_path: Option<PathBuf>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedListJobsPage {
    limit: usize,
    offset: usize,
    order: ListJobsOrderInput,
    detail: ListJobsDetailInput,
    query_limit: usize,
}

#[tool_router]
impl GflowMcpServer {
    fn new(config_path: Option<PathBuf>) -> Self {
        Self {
            config_path,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Read scheduler and GPU status from the local gflow daemon.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SchedulerInfoOutput>()
    )]
    async fn get_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let info = client.get_info().await.map_err(stringify_error)?;
        structured_response(info)
    }

    #[tool(
        description = "Check whether the local gflow daemon is running and responsive.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<HealthOutput>()
    )]
    async fn get_health(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let status = client.get_health().await.map_err(stringify_error)?;
        let pid = client
            .get_health_with_pid()
            .await
            .map_err(stringify_error)?;

        structured_response(json!({
            "status": status.as_u16(),
            "ok": status.is_success(),
            "pid": pid,
        }))
    }

    #[tool(
        description = "List jobs from the local gflow daemon. Defaults to recent jobs first.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ListJobsOutput>()
    )]
    async fn list_jobs(
        &self,
        Parameters(params): Parameters<ListJobsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let page = resolve_list_jobs_page(&params);
        let jobs = client
            .list_jobs_with_query(
                params.state,
                params.user,
                Some(page.query_limit),
                Some(page.offset),
                params.created_after,
                Some(page.order.as_query_value().to_string()),
            )
            .await
            .map_err(stringify_error)?;
        let mut jobs = jobs;
        let has_more = jobs.len() > page.limit;
        if has_more {
            jobs.truncate(page.limit);
        }
        let jobs = jobs
            .into_iter()
            .map(|job| serialize_list_job(job, page.detail))
            .collect::<Vec<_>>();
        let count = jobs.len();
        structured_response(json!({
            "jobs": jobs,
            "count": count,
            "detail": page.detail,
            "limit": page.limit,
            "offset": page.offset,
            "has_more": has_more,
            "next_offset": if has_more {
                Some(page.offset.saturating_add(count))
            } else {
                None::<usize>
            },
        }))
    }

    #[tool(
        description = "Get a single job by ID.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArbitraryObjectSchema>()
    )]
    async fn get_job(
        &self,
        Parameters(JobIdRequest { job_id }): Parameters<JobIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let job = client
            .get_job(job_id)
            .await
            .map_err(stringify_error)?
            .ok_or_else(|| stringify_error(anyhow::anyhow!("Job {job_id} not found")))?;
        structured_response(job)
    }

    #[tool(
        description = "Read the local log file for a job.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<JobLogOutput>()
    )]
    async fn get_job_log(
        &self,
        Parameters(params): Parameters<GetJobLogRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let job = client
            .get_job(params.job_id)
            .await
            .map_err(stringify_error)?
            .ok_or_else(|| stringify_error(anyhow::anyhow!("Job {} not found", params.job_id)))?;
        let log_path = client
            .get_job_log_path(params.job_id)
            .await
            .map_err(stringify_error)?
            .ok_or_else(|| {
                stringify_error(anyhow::anyhow!(
                    "Job {} does not have a log path yet",
                    params.job_id
                ))
            })?;

        let raw = fs::read_to_string(&log_path).map_err(|err| {
            stringify_error(anyhow::anyhow!(
                "Failed to read log file '{}': {}",
                log_path,
                err
            ))
        })?;
        let slice = resolve_log_slice(&params).map_err(stringify_error)?;
        let cleaned = slice_text(clean_terminal_output(&raw), slice, params.max_bytes);
        let program_output = extract_likely_program_output(&cleaned, &job);

        structured_response(json!({
            "job_id": params.job_id,
            "log_path": log_path,
            "text": if program_output.is_empty() { cleaned.clone() } else { program_output.clone() },
            "program_output": if program_output.is_empty() { Value::Null } else { json!(program_output) },
            "full_text": cleaned,
        }))
    }

    #[tool(
        description = "Read usage statistics from the local gflow daemon.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<UsageStatsOutput>()
    )]
    async fn get_stats(
        &self,
        Parameters(params): Parameters<GetStatsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let stats = client
            .get_stats(params.user.as_deref(), params.since)
            .await
            .map_err(stringify_error)?;
        structured_response(stats)
    }

    #[tool(
        description = "Summarize queue pressure and GPU availability for agent planning. Read-only.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<QueuePressureOutput>()
    )]
    async fn get_queue_pressure(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let info = client.get_info().await.map_err(stringify_error)?;
        let jobs = client
            .list_jobs_with_query(
                Some("Queued,Hold,Running".to_string()),
                None,
                None,
                None,
                None,
                Some("asc".to_string()),
            )
            .await
            .map_err(stringify_error)?;
        let reservations = client
            .list_reservations(None, None, false)
            .await
            .map_err(stringify_error)?;

        let output = build_queue_pressure_output(info, jobs, reservations);
        structured_response(output)
    }

    #[tool(
        description = "List GPU reservations from the local gflow daemon.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ListReservationsOutput>()
    )]
    async fn list_reservations(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let reservations = client
            .list_reservations(None, None, false)
            .await
            .map_err(stringify_error)?;
        let count = reservations.len();
        structured_response(json!({
            "reservations": reservations,
            "count": count,
        }))
    }

    #[tool(
        description = "MUTATES scheduler state: cancel a job through the local gflow daemon. Caller should require explicit user confirmation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<JobActionOutput>()
    )]
    async fn cancel_job(
        &self,
        Parameters(JobIdRequest { job_id }): Parameters<JobIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        client.cancel_job(job_id).await.map_err(stringify_error)?;
        structured_response(json!({ "job_id": job_id, "cancelled": true }))
    }

    #[tool(
        description = "MUTATES scheduler state: put a queued job on hold through the local gflow daemon. Caller should require explicit user confirmation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<JobActionOutput>()
    )]
    async fn hold_job(
        &self,
        Parameters(JobIdRequest { job_id }): Parameters<JobIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        client.hold_job(job_id).await.map_err(stringify_error)?;
        structured_response(json!({ "job_id": job_id, "held": true }))
    }

    #[tool(
        description = "MUTATES scheduler state: release a held job through the local gflow daemon. Caller should require explicit user confirmation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<JobActionOutput>()
    )]
    async fn release_job(
        &self,
        Parameters(JobIdRequest { job_id }): Parameters<JobIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        client.release_job(job_id).await.map_err(stringify_error)?;
        structured_response(json!({ "job_id": job_id, "released": true }))
    }

    #[tool(
        description = "Preview one or more job submissions without creating jobs. Read-only dry run using the same simplified schema as submit_jobs.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PreviewSubmitJobOutput>()
    )]
    async fn preview_submit_jobs(
        &self,
        Parameters(params): Parameters<SubmitJobsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let input_count = params.jobs.len();
        let output = preview_submit_jobs_output(params.jobs, input_count);
        structured_response(output)
    }

    #[tool(
        description = "MUTATES scheduler state: submit one or more jobs to the local gflow daemon using a simplified schema. Prefer preview_submit_jobs first and require explicit user confirmation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SubmitJobsOutput>()
    )]
    async fn submit_jobs(
        &self,
        Parameters(params): Parameters<SubmitJobsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let expanded_jobs = expand_submit_job_requests(params.jobs)
            .map_err(|err| stringify_error(anyhow::anyhow!(err)))?;
        if expanded_jobs.len() > 1000 {
            return Err(stringify_error(anyhow::anyhow!(
                "submit_jobs accepts at most 1000 jobs"
            )));
        }

        let mut results = Vec::with_capacity(expanded_jobs.len());
        let mut submitted = 0usize;

        for (index, params) in expanded_jobs {
            match build_submit_job(params) {
                Ok(job) => match client.add_job(job).await {
                    Ok(response) => {
                        submitted += 1;
                        results.push(SubmitJobResultOutput {
                            index,
                            ok: true,
                            job_id: Some(response.id),
                            run_name: Some(response.run_name),
                            error: None,
                        });
                    }
                    Err(err) => {
                        results.push(SubmitJobResultOutput {
                            index,
                            ok: false,
                            job_id: None,
                            run_name: None,
                            error: Some(err.to_string()),
                        });
                    }
                },
                Err(err) => {
                    results.push(SubmitJobResultOutput {
                        index,
                        ok: false,
                        job_id: None,
                        run_name: None,
                        error: Some(err),
                    });
                }
            }
        }

        let failed = results.len().saturating_sub(submitted);
        structured_response(SubmitJobsOutput {
            results,
            submitted,
            failed,
        })
    }

    #[tool(
        description = "Preview mutable job parameter changes without updating scheduler state. Read-only dry run.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PreviewUpdateJobOutput>()
    )]
    async fn preview_update_job(
        &self,
        Parameters(params): Parameters<UpdateJobToolRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let job_id = params.job_id;
        let job = client
            .get_job(job_id)
            .await
            .map_err(stringify_error)?
            .ok_or_else(|| stringify_error(anyhow::anyhow!("Job {job_id} not found")))?;
        let request =
            build_update_request(params).map_err(|err| stringify_error(anyhow::anyhow!(err)))?;
        let output = preview_update_job_output(job, request);
        structured_response(output)
    }

    #[tool(
        description = "MUTATES scheduler state: update mutable job parameters on the local gflow daemon. Prefer preview_update_job first and require explicit user confirmation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<UpdateJobOutputSchema>()
    )]
    async fn update_job(
        &self,
        Parameters(params): Parameters<UpdateJobToolRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let job_id = params.job_id;
        let request =
            build_update_request(params).map_err(|err| stringify_error(anyhow::anyhow!(err)))?;
        let response: UpdateJobResponse = client
            .update_job(job_id, request)
            .await
            .map_err(stringify_error)?;
        structured_response(response)
    }

    #[tool(
        description = "Summarize why a job is queued or failed, including recent log evidence and retry hints. Read-only.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TriageJobOutput>()
    )]
    async fn triage_job(
        &self,
        Parameters(params): Parameters<TriageJobRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let job = client
            .get_job(params.job_id)
            .await
            .map_err(stringify_error)?
            .ok_or_else(|| stringify_error(anyhow::anyhow!("Job {} not found", params.job_id)))?;
        let (log_path, log_excerpt) = read_job_log_excerpt(&client, &job, &params)
            .await
            .map_err(stringify_error)?;
        let output = build_triage_job_output(job, log_path, log_excerpt)
            .map_err(|err| stringify_error(anyhow::anyhow!(err)))?;
        structured_response(output)
    }

    #[tool(
        description = "MUTATES scheduler state: resubmit a finished job with the same or overridden parameters, optionally cascading to dependency-cancelled child jobs. Prefer triage_job first and require explicit user confirmation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RedoJobOutput>()
    )]
    async fn redo_job(
        &self,
        Parameters(params): Parameters<RedoJobRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let options = crate::multicall::gjob::commands::redo::RedoJobOptions {
            gpus_override: params.gpus,
            priority_override: params.priority,
            depends_on_override: params.depends_on,
            time_limit_override: params.time_limit_secs.map(Duration::from_secs),
            memory_limit_mb_override: params.memory_limit_mb,
            gpu_memory_limit_mb_override: params.gpu_memory_limit_mb,
            conda_env_override: params.conda_env,
            clear_deps: params.clear_deps.unwrap_or(false),
            cascade: params.cascade.unwrap_or(false),
        };
        let result =
            crate::multicall::gjob::commands::redo::redo_job(&client, params.job_id, &options)
                .await
                .map_err(stringify_error)?;
        let cascaded_count = result.cascaded_jobs.len();
        let cascaded_jobs = result
            .cascaded_jobs
            .into_iter()
            .map(|job| {
                json!({
                    "original_job_id": job.original_job_id,
                    "new_job_id": job.new_job_id,
                    "run_name": job.run_name,
                })
            })
            .collect::<Vec<_>>();

        structured_response(json!({
            "original_job_id": result.original_job_id,
            "new_job_id": result.new_job_id,
            "run_name": result.run_name,
            "cascaded_jobs": cascaded_jobs,
            "cascaded_count": cascaded_count,
        }))
    }
}

#[tool_handler]
impl rmcp::ServerHandler for GflowMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Local-first gflow MCP server. Prefer read-only tools and preview_* dry runs before mutating scheduler state. Tools marked MUTATES require explicit caller-side user confirmation."
                .to_string(),
        )
    }
}

impl GflowMcpServer {
    fn client(&self) -> anyhow::Result<Client> {
        gflow::create_client(&self.config_path)
    }
}

fn resolve_list_jobs_page(params: &ListJobsRequest) -> ResolvedListJobsPage {
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

fn serialize_list_job(job: Job, detail: ListJobsDetailInput) -> Value {
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

fn system_time_to_unix_secs(ts: SystemTime) -> Option<u64> {
    ts.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

fn preview_submit_jobs_output(
    jobs: Vec<SubmitJobRequest>,
    input_count: usize,
) -> PreviewSubmitJobOutput {
    let warnings = vec![
        "dry run only validates MCP-side request shape; daemon-side dependency, cycle, and project policy checks still run at submission time".to_string(),
    ];

    let expanded_jobs = match expand_submit_job_requests(jobs) {
        Ok(expanded_jobs) => expanded_jobs,
        Err(error) => {
            return PreviewSubmitJobOutput {
                dry_run: true,
                valid: false,
                input_count,
                expanded_count: 0,
                jobs: vec![PreviewSubmitJobResultOutput {
                    input_index: 0,
                    expanded_index: 0,
                    ok: false,
                    job: None,
                    error: Some(error),
                    warnings: Vec::new(),
                }],
                warnings,
            };
        }
    };

    let mut results = Vec::with_capacity(expanded_jobs.len());
    for (expanded_index, (input_index, params)) in expanded_jobs.into_iter().enumerate() {
        match build_submit_job(params) {
            Ok(job) => {
                let job_warnings = preview_submit_warnings(&job);
                results.push(PreviewSubmitJobResultOutput {
                    input_index,
                    expanded_index,
                    ok: true,
                    job: Some(serialize_job_value(&job)),
                    error: None,
                    warnings: job_warnings,
                });
            }
            Err(error) => {
                results.push(PreviewSubmitJobResultOutput {
                    input_index,
                    expanded_index,
                    ok: false,
                    job: None,
                    error: Some(error),
                    warnings: Vec::new(),
                });
            }
        }
    }

    let valid = results.iter().all(|result| result.ok);
    PreviewSubmitJobOutput {
        dry_run: true,
        valid,
        input_count,
        expanded_count: results.len(),
        jobs: results,
        warnings,
    }
}

fn preview_submit_warnings(job: &Job) -> Vec<String> {
    let mut warnings = Vec::new();
    if job.depends_on.is_some() || !job.depends_on_ids.is_empty() {
        warnings.push(
            "dependency existence and circular dependency checks require the daemon submit path"
                .to_string(),
        );
    }
    if job.project.is_some() {
        warnings.push("project policy validation requires the daemon submit path".to_string());
    }
    warnings
}

fn preview_update_job_output(job: Job, request: UpdateJobRequest) -> PreviewUpdateJobOutput {
    let before = serialize_job_value(&job);
    let job_id = job.id;
    let mut warnings = vec![
        "dry run does not validate dependency existence or circular dependency changes".to_string(),
    ];

    if !matches!(job.state, JobState::Queued | JobState::Hold) {
        return PreviewUpdateJobOutput {
            dry_run: true,
            ok: false,
            job_id,
            before: Some(before),
            after: None,
            updated_fields: Vec::new(),
            error: Some(format!(
                "Job {} is in state '{}' and cannot be updated. Only queued or held jobs can be updated.",
                job_id, job.state
            )),
            warnings,
        };
    }

    if job.gpu_sharing_mode == GpuSharingMode::Shared
        && matches!(request.gpu_memory_limit_mb, Some(None))
    {
        return PreviewUpdateJobOutput {
            dry_run: true,
            ok: false,
            job_id,
            before: Some(before),
            after: None,
            updated_fields: Vec::new(),
            error: Some(
                "Shared jobs must keep a GPU memory limit (--gpu-memory / --max-gpu-mem)."
                    .to_string(),
            ),
            warnings,
        };
    }

    let (after_job, updated_fields) = apply_update_preview(job, request);
    if !updated_fields
        .iter()
        .any(|field| matches!(field.as_str(), "depends_on_ids" | "dependency_mode"))
    {
        warnings.clear();
    }

    PreviewUpdateJobOutput {
        dry_run: true,
        ok: true,
        job_id,
        before: Some(before),
        after: Some(serialize_job_value(&after_job)),
        updated_fields,
        error: None,
        warnings,
    }
}

fn apply_update_preview(mut job: Job, request: UpdateJobRequest) -> (Job, Vec<String>) {
    let mut updated_fields = Vec::new();

    if let Some(command) = request.command {
        job.command = Some(CompactString::from(command));
        updated_fields.push("command".to_string());
    }
    if let Some(script) = request.script {
        job.script = Some(Box::new(script));
        updated_fields.push("script".to_string());
    }
    if let Some(gpus) = request.gpus {
        job.gpus = gpus;
        updated_fields.push("gpus".to_string());
    }
    if let Some(conda_env) = request.conda_env {
        job.conda_env = conda_env.map(CompactString::from);
        updated_fields.push("conda_env".to_string());
    }
    if let Some(priority) = request.priority {
        job.priority = priority;
        updated_fields.push("priority".to_string());
    }
    if let Some(parameters) = request.parameters {
        job.parameters = parameters
            .into_iter()
            .map(|(key, value)| (CompactString::from(key), CompactString::from(value)))
            .collect();
        updated_fields.push("parameters".to_string());
    }
    if let Some(time_limit) = request.time_limit {
        job.time_limit = time_limit;
        updated_fields.push("time_limit".to_string());
    }
    if let Some(memory_limit_mb) = request.memory_limit_mb {
        job.memory_limit_mb = memory_limit_mb;
        updated_fields.push("memory_limit_mb".to_string());
    }
    if let Some(gpu_memory_limit_mb) = request.gpu_memory_limit_mb {
        job.gpu_memory_limit_mb = gpu_memory_limit_mb;
        updated_fields.push("gpu_memory_limit_mb".to_string());
    }
    if let Some(depends_on_ids) = request.depends_on_ids {
        job.depends_on_ids = depends_on_ids.into();
        updated_fields.push("depends_on_ids".to_string());
    }
    if let Some(dependency_mode) = request.dependency_mode {
        job.dependency_mode = dependency_mode;
        updated_fields.push("dependency_mode".to_string());
    }
    if let Some(auto_cancel) = request.auto_cancel_on_dependency_failure {
        job.auto_cancel_on_dependency_failure = auto_cancel;
        updated_fields.push("auto_cancel_on_dependency_failure".to_string());
    }
    if let Some(max_concurrent) = request.max_concurrent {
        job.max_concurrent = max_concurrent;
        updated_fields.push("max_concurrent".to_string());
    }
    if let Some(max_retries) = request.max_retries {
        job.max_retries = max_retries.unwrap_or(0);
        updated_fields.push("max_retries".to_string());
    }
    if let Some(notifications) = request.notifications {
        job.notifications = notifications;
        updated_fields.push("notifications".to_string());
    }

    (job, updated_fields)
}

fn serialize_job_value(job: &Job) -> Value {
    serde_json::to_value(job).unwrap_or_else(|err| {
        json!({
            "error": format!("Failed to serialize job: {}", err),
        })
    })
}

async fn read_job_log_excerpt(
    client: &Client,
    job: &Job,
    params: &TriageJobRequest,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    let Some(log_path) = client.get_job_log_path(params.job_id).await? else {
        return Ok((None, None));
    };

    let raw = match fs::read_to_string(&log_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Some(log_path), None));
        }
        Err(err) => {
            anyhow::bail!("Failed to read log file '{}': {}", log_path, err);
        }
    };

    let slice = TextSlice::Last(params.last_lines.unwrap_or(80));
    let cleaned = slice_text(
        clean_terminal_output(&raw),
        slice,
        Some(params.max_bytes.unwrap_or(20_000)),
    );
    let program_output = extract_likely_program_output(&cleaned, job);
    let excerpt = if program_output.is_empty() {
        cleaned
    } else {
        program_output
    };

    Ok((Some(log_path), Some(excerpt)))
}

fn build_triage_job_output(
    job: Job,
    log_path: Option<String>,
    log_excerpt: Option<String>,
) -> anyhow::Result<TriageJobOutput> {
    let retry_hints = retry_hints_for_job(&job, log_excerpt.as_deref());
    let output = TriageJobOutput {
        job_id: job.id,
        state: job.state.to_string(),
        reason: job.reason.as_deref().map(ToString::to_string),
        requested_gpus: job.gpus,
        gpu_ids: job.gpu_ids.as_ref().map(|ids| ids.to_vec()),
        runtime_secs: job_runtime_secs(&job),
        wait_secs: job_wait_secs(&job),
        exit_status: None,
        exit_status_note: "gflow currently records terminal state but not the process exit code"
            .to_string(),
        log_path,
        log_excerpt,
        retry_hints,
        job: serialize_job_value(&job),
    };
    Ok(output)
}

fn job_runtime_secs(job: &Job) -> Option<f64> {
    let start = job.started_at?;
    let end = job.finished_at.unwrap_or_else(SystemTime::now);
    duration_between_secs(start, end)
}

fn job_wait_secs(job: &Job) -> Option<f64> {
    let submitted = job.submitted_at?;
    let end = job.started_at.unwrap_or_else(SystemTime::now);
    duration_between_secs(submitted, end)
}

fn duration_between_secs(start: SystemTime, end: SystemTime) -> Option<f64> {
    end.duration_since(start)
        .ok()
        .map(|duration| duration.as_secs_f64())
}

fn retry_hints_for_job(job: &Job, log_excerpt: Option<&str>) -> Vec<String> {
    let mut hints = Vec::new();

    match job.state {
        JobState::Queued => match job.reason.as_deref().map(ToString::to_string) {
            Some(reason) if reason.contains("Dependency") => hints.push(
                "inspect dependency jobs before retrying or changing dependencies".to_string(),
            ),
            Some(reason) if reason.contains("Memory") => {
                hints.push("lower memory request or wait for memory pressure to clear".to_string())
            }
            Some(reason) if reason.contains("Gpu") || reason.contains("Resources") => hints.push(
                "check get_queue_pressure for GPU availability, reservations, and running jobs"
                    .to_string(),
            ),
            _ => hints.push("check get_queue_pressure before changing the job".to_string()),
        },
        JobState::Failed | JobState::Timeout => {
            hints.push("review the log excerpt before using redo_job".to_string());
            if job.max_retries > 0 {
                hints.push(format!(
                    "job has max_retries={} configured; check whether automatic retries already ran",
                    job.max_retries
                ));
            }
        }
        JobState::Cancelled => {
            hints.push("confirm why the job was cancelled before resubmitting".to_string());
        }
        JobState::Hold => {
            hints.push(
                "release_job can make this job schedulable after user confirmation".to_string(),
            );
        }
        JobState::Running => {
            hints.push("job is still running; inspect logs instead of retrying".to_string());
        }
        JobState::Finished => {
            hints.push("job finished successfully; retry is usually unnecessary".to_string());
        }
    }

    if let Some(log) = log_excerpt {
        let lower = log.to_ascii_lowercase();
        if lower.contains("out of memory") || lower.contains("cuda oom") {
            hints.push(
                "log suggests OOM; consider requesting more memory or reducing workload"
                    .to_string(),
            );
        }
        if lower.contains("no space left") {
            hints.push("log suggests disk pressure; free space before retrying".to_string());
        }
        if lower.contains("command not found") {
            hints.push("log suggests environment or PATH setup failure".to_string());
        }
    }

    hints.sort();
    hints.dedup();
    hints
}

#[derive(Debug, Default)]
struct QueuePressureGroupAccumulator {
    queued: usize,
    running: usize,
    requested_gpus: u32,
}

fn build_queue_pressure_output(
    info: gflow::core::info::SchedulerInfo,
    jobs: Vec<Job>,
    reservations: Vec<gflow::core::reservation::GpuReservation>,
) -> QueuePressureOutput {
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let available_gpus = info
        .gpus
        .iter()
        .filter(|gpu| gpu.available)
        .map(|gpu| gpu.index)
        .collect::<Vec<_>>();
    let unavailable_gpus = info
        .gpus
        .iter()
        .filter(|gpu| !gpu.available)
        .map(|gpu| GpuAvailabilityOutput {
            index: gpu.index,
            reason: gpu.reason.clone(),
        })
        .collect::<Vec<_>>();

    let mut running_jobs = 0usize;
    let mut queued_jobs = 0usize;
    let mut held_jobs = 0usize;
    let mut queued_requested_gpus = 0u32;
    let mut running_allocated_gpus = 0u32;
    let mut blocked_reasons = BTreeMap::new();
    let mut users = BTreeMap::<String, QueuePressureGroupAccumulator>::new();
    let mut projects = BTreeMap::<String, QueuePressureGroupAccumulator>::new();

    for job in &jobs {
        match job.state {
            JobState::Running => {
                running_jobs += 1;
                running_allocated_gpus += job
                    .gpu_ids
                    .as_ref()
                    .map(|ids| ids.len() as u32)
                    .unwrap_or(job.gpus);
                accumulate_queue_group(&mut users, job.submitted_by.as_ref(), job, false);
                if let Some(project) = &job.project {
                    accumulate_queue_group(&mut projects, project.as_ref(), job, false);
                }
            }
            JobState::Queued => {
                queued_jobs += 1;
                queued_requested_gpus = queued_requested_gpus.saturating_add(job.gpus);
                *blocked_reasons.entry(job_reason_label(job)).or_insert(0) += 1;
                accumulate_queue_group(&mut users, job.submitted_by.as_ref(), job, true);
                if let Some(project) = &job.project {
                    accumulate_queue_group(&mut projects, project.as_ref(), job, true);
                }
            }
            JobState::Hold => {
                held_jobs += 1;
                *blocked_reasons.entry(job_reason_label(job)).or_insert(0) += 1;
                accumulate_queue_group(&mut users, job.submitted_by.as_ref(), job, true);
                if let Some(project) = &job.project {
                    accumulate_queue_group(&mut projects, project.as_ref(), job, true);
                }
            }
            _ => {}
        }
    }

    let now = SystemTime::now();
    let reservations_active = reservations
        .iter()
        .filter(|reservation| {
            reservation.status == ReservationStatus::Active || reservation.is_active(now)
        })
        .count();

    QueuePressureOutput {
        generated_at,
        total_gpus: info.gpus.len(),
        available_gpus,
        unavailable_gpus,
        running_jobs,
        queued_jobs,
        held_jobs,
        queued_requested_gpus,
        running_allocated_gpus,
        blocked_reasons,
        users: queue_group_outputs(users),
        projects: queue_group_outputs(projects),
        reservations_total: reservations.len(),
        reservations_active,
    }
}

fn accumulate_queue_group(
    groups: &mut BTreeMap<String, QueuePressureGroupAccumulator>,
    name: &str,
    job: &Job,
    queued: bool,
) {
    let group = groups.entry(name.to_string()).or_default();
    if queued {
        group.queued += 1;
        group.requested_gpus = group.requested_gpus.saturating_add(job.gpus);
    } else {
        group.running += 1;
    }
}

fn queue_group_outputs(
    groups: BTreeMap<String, QueuePressureGroupAccumulator>,
) -> Vec<QueuePressureGroupOutput> {
    let mut outputs = groups
        .into_iter()
        .map(|(name, group)| QueuePressureGroupOutput {
            name,
            queued: group.queued,
            running: group.running,
            requested_gpus: group.requested_gpus,
        })
        .collect::<Vec<_>>();
    outputs.sort_by(|left, right| {
        right
            .requested_gpus
            .cmp(&left.requested_gpus)
            .then_with(|| right.queued.cmp(&left.queued))
            .then_with(|| left.name.cmp(&right.name))
    });
    outputs.truncate(10);
    outputs
}

fn job_reason_label(job: &Job) -> String {
    if let Some(reason) = job.reason.as_deref() {
        return reason.to_string();
    }

    match job.state {
        JobState::Hold => "JobHeldUser".to_string(),
        JobState::Queued => "Resources".to_string(),
        _ => "unknown".to_string(),
    }
}

pub async fn run(config_path: Option<PathBuf>, verbosity: Verbosity) -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_max_level(verbosity)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();

    let server = GflowMcpServer::new(config_path);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn build_submit_job(params: SubmitJobRequest) -> Result<Job, String> {
    if params.command.is_none() && params.script.is_none() {
        return Err("submit_job requires either 'command' or 'script'".to_string());
    }
    if params.command.is_some() && params.script.is_some() {
        return Err("submit_job accepts either 'command' or 'script', not both".to_string());
    }
    if params.shared.unwrap_or(false) && params.gpu_memory_limit_mb.is_none() {
        return Err("submit_job requires 'gpu_memory_limit_mb' when 'shared' is true".to_string());
    }

    let mut builder = JobBuilder::new()
        .gpus(params.gpus.unwrap_or(0))
        .run_dir(
            params
                .run_dir
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into())),
        )
        .priority(params.priority.unwrap_or(10))
        .submitted_by(
            params
                .submitted_by
                .unwrap_or_else(resolve_default_submitted_by),
        )
        .auto_close_tmux(params.auto_close_tmux.unwrap_or(false))
        .shared(params.shared.unwrap_or(false))
        .max_concurrent(params.max_concurrent)
        .max_retries(params.max_retries.unwrap_or(0))
        .run_name(params.run_name)
        .project(params.project);

    if let Some(notifications) =
        resolve_job_notifications(params.notify_email, params.notify_on, "submit_job")?
    {
        builder = builder.notifications(notifications);
    }

    if let Some(command) = params.command {
        builder = builder.command(command);
    }
    if let Some(script) = params.script {
        builder = builder.script(script);
    }
    if let Some(conda_env) = params.conda_env {
        builder = builder.conda_env(Some(conda_env));
    }
    if let Some(depends_on) = params.depends_on {
        builder = builder.depends_on(Some(depends_on));
    }
    if let Some(depends_on_ids) = params.depends_on_ids {
        builder = builder.depends_on_ids(depends_on_ids);
    }
    if let Some(dependency_mode) = params.dependency_mode {
        builder = builder.dependency_mode(Some(dependency_mode.into()));
    }
    if let Some(auto_cancel) = params.auto_cancel_on_dependency_failure {
        builder = builder.auto_cancel_on_dependency_failure(auto_cancel);
    }
    if let Some(gpu_memory_limit_mb) = params.gpu_memory_limit_mb {
        builder = builder.gpu_memory_limit_mb(Some(gpu_memory_limit_mb));
    }
    if let Some(time_limit_secs) = params.time_limit_secs {
        builder = builder.time_limit(Some(Duration::from_secs(time_limit_secs)));
    }
    if let Some(memory_limit_mb) = params.memory_limit_mb {
        builder = builder.memory_limit_mb(Some(memory_limit_mb));
    }
    if let Some(parameters) = params.parameters {
        builder = builder.parameters(parameters);
    }

    Ok(builder.build())
}

fn expand_submit_job_requests(
    jobs: Vec<SubmitJobRequest>,
) -> Result<Vec<(usize, SubmitJobRequest)>, String> {
    let mut expanded = Vec::new();

    for (index, job) in jobs.into_iter().enumerate() {
        expanded.extend(expand_single_submit_job_request(index, job)?);
    }

    Ok(expanded)
}

fn expand_single_submit_job_request(
    index: usize,
    job: SubmitJobRequest,
) -> Result<Vec<(usize, SubmitJobRequest)>, String> {
    let Some(param_specs_raw) = job.param.clone().filter(|params| !params.is_empty()) else {
        return Ok(vec![(index, job)]);
    };

    let mut parsed_specs = Vec::with_capacity(param_specs_raw.len());
    for spec in &param_specs_raw {
        parsed_specs.push(parse_param_spec(spec).map_err(|err| err.to_string())?);
    }

    let param_combinations = generate_param_combinations(&parsed_specs);
    let mut expanded_jobs = Vec::with_capacity(param_combinations.len());

    for combination in param_combinations {
        let mut expanded_job = job.clone();
        expanded_job.param = None;

        let mut parameters = expanded_job.parameters.take().unwrap_or_default();
        for (key, value) in combination {
            if parameters.contains_key(&key) {
                return Err(format!(
                    "submit_job cannot use the same key in both 'parameters' and 'param': {}",
                    key
                ));
            }
            parameters.insert(key, value);
        }

        expanded_job.parameters = if parameters.is_empty() {
            None
        } else {
            Some(parameters)
        };
        expanded_jobs.push((index, expanded_job));
    }

    Ok(expanded_jobs)
}

fn build_update_request(params: UpdateJobToolRequest) -> Result<UpdateJobRequest, String> {
    if params.conda_env.is_some() && params.clear_conda_env.unwrap_or(false) {
        return Err("Cannot set and clear conda_env in the same request".to_string());
    }
    if params.time_limit_secs.is_some() && params.clear_time_limit.unwrap_or(false) {
        return Err("Cannot set and clear time_limit in the same request".to_string());
    }
    if params.memory_limit_mb.is_some() && params.clear_memory_limit.unwrap_or(false) {
        return Err("Cannot set and clear memory_limit_mb in the same request".to_string());
    }
    if params.gpu_memory_limit_mb.is_some() && params.clear_gpu_memory_limit.unwrap_or(false) {
        return Err("Cannot set and clear gpu_memory_limit_mb in the same request".to_string());
    }
    if params.dependency_mode.is_some() && params.clear_dependency_mode.unwrap_or(false) {
        return Err("Cannot set and clear dependency_mode in the same request".to_string());
    }
    if params.max_concurrent.is_some() && params.clear_max_concurrent.unwrap_or(false) {
        return Err("Cannot set and clear max_concurrent in the same request".to_string());
    }
    if params.max_retries.is_some() && params.clear_max_retries.unwrap_or(false) {
        return Err("Cannot set and clear max_retries in the same request".to_string());
    }

    let notifications =
        resolve_job_notifications(params.notify_email, params.notify_on, "update_job")?;

    Ok(UpdateJobRequest {
        command: params.command,
        script: params.script.map(PathBuf::from),
        gpus: params.gpus,
        conda_env: match (params.conda_env, params.clear_conda_env.unwrap_or(false)) {
            (Some(env), false) => Some(Some(env)),
            (None, true) => Some(None),
            _ => None,
        },
        priority: params.priority,
        parameters: params.parameters,
        time_limit: match (
            params.time_limit_secs,
            params.clear_time_limit.unwrap_or(false),
        ) {
            (Some(secs), false) => Some(Some(Duration::from_secs(secs))),
            (None, true) => Some(None),
            _ => None,
        },
        memory_limit_mb: match (
            params.memory_limit_mb,
            params.clear_memory_limit.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        gpu_memory_limit_mb: match (
            params.gpu_memory_limit_mb,
            params.clear_gpu_memory_limit.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        depends_on_ids: params.depends_on_ids,
        dependency_mode: match (
            params.dependency_mode,
            params.clear_dependency_mode.unwrap_or(false),
        ) {
            (Some(mode), false) => Some(Some(mode.into())),
            (None, true) => Some(None),
            _ => None,
        },
        auto_cancel_on_dependency_failure: params.auto_cancel_on_dependency_failure,
        max_concurrent: match (
            params.max_concurrent,
            params.clear_max_concurrent.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        max_retries: match (
            params.max_retries,
            params.clear_max_retries.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        notifications,
    })
}

fn resolve_job_notifications(
    notify_email: Option<Vec<String>>,
    notify_on: Option<Vec<String>>,
    context: &str,
) -> Result<Option<JobNotifications>, String> {
    let Some(emails) = notify_email else {
        if notify_on.is_some() {
            return Err(format!(
                "{context} requires 'notify_email' when 'notify_on' is set"
            ));
        }
        return Ok(None);
    };

    for email in &emails {
        email.parse::<Mailbox>().map_err(|err| {
            format!(
                "{context} received invalid email recipient '{}': {err}",
                email
            )
        })?;
    }

    if emails.is_empty() && notify_on.as_ref().is_some_and(|events| !events.is_empty()) {
        return Err(format!(
            "{context} cannot use 'notify_on' with an empty 'notify_email' list"
        ));
    }

    Ok(Some(JobNotifications::normalized(
        emails,
        notify_on.unwrap_or_default(),
    )))
}

fn resolve_default_submitted_by() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn structured_response<T: serde::Serialize>(value: T) -> Result<CallToolResult, rmcp::ErrorData> {
    let value = serde_json::to_value(value).map_err(|err| {
        rmcp::ErrorData::internal_error(format!("Failed to serialize MCP response: {}", err), None)
    })?;

    Ok(CallToolResult::structured(value))
}

fn stringify_error(err: anyhow::Error) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(err.to_string(), None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextSlice {
    Full,
    First(usize),
    Last(usize),
}

fn resolve_log_slice(params: &GetJobLogRequest) -> anyhow::Result<TextSlice> {
    match (params.first_lines, params.last_lines) {
        (Some(_), Some(_)) => {
            anyhow::bail!("get_job_log accepts only one of first_lines or last_lines")
        }
        (Some(lines), None) => Ok(TextSlice::First(lines)),
        (None, Some(lines)) => Ok(TextSlice::Last(lines)),
        (None, None) => Ok(TextSlice::Full),
    }
}

fn slice_text(text: String, slice: TextSlice, max_bytes: Option<usize>) -> String {
    let mut output = text;

    match slice {
        TextSlice::Full => {}
        TextSlice::First(first_lines) => {
            output = output
                .lines()
                .take(first_lines)
                .collect::<Vec<_>>()
                .join("\n");
        }
        TextSlice::Last(last_lines) => {
            let lines: Vec<_> = output.lines().collect();
            output = lines
                .into_iter()
                .rev()
                .take(last_lines)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
        }
    }

    if let Some(max_bytes) = max_bytes {
        let bytes = output.as_bytes();
        if bytes.len() > max_bytes {
            output = String::from_utf8_lossy(&bytes[bytes.len() - max_bytes..]).to_string();
        }
    }

    output
}

#[allow(clippy::while_let_loop, clippy::while_let_on_iterator)]
fn clean_terminal_output(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.peek().copied() {
                Some(']') => {
                    chars.next();
                    loop {
                        let Some(next) = chars.next() else {
                            break;
                        };
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && matches!(chars.peek(), Some('\\')) {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('[') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => break,
            }
            continue;
        }

        if ch == '\r' {
            continue;
        }

        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }

        output.push(ch);
    }

    output
        .lines()
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn extract_likely_program_output(text: &str, job: &Job) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !is_shell_noise_line(line))
        .filter(|line| !is_internal_gflow_line(line, job.id))
        .filter(|line| !is_wrapped_user_command_line(line, job))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn is_shell_noise_line(line: &str) -> bool {
    line.starts_with("cd ")
        || line.starts_with("export GFLOW_ARRAY_TASK_ID=")
        || line.starts_with("export CUDA_VISIBLE_DEVICES=")
        || line.starts_with("conda activate ")
        || line.starts_with("➜ ")
        || line == "✗"
        || line.starts_with('¶')
        || line.contains("[$?] is")
        || line.contains(" via ")
        || line.contains('…')
}

fn is_internal_gflow_line(line: &str, job_id: u32) -> bool {
    line.contains("target/debug/gflow __multicall gcancel")
        || line.contains("Running `target/debug/gflow __multicall gcancel")
        || line.contains("Finished `dev` profile")
        || line.contains(&format!("gcancel --finish {job_id}"))
        || line.contains(&format!("gcancel --fail {job_id}"))
}

fn is_wrapped_user_command_line(line: &str, job: &Job) -> bool {
    if line.starts_with("bash -c ") {
        return true;
    }

    if let Some(command) = &job.command {
        let normalized_command = command.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized_line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized_line.contains(&normalized_command)
            || normalized_line.contains(&normalized_command.replace('"', "\\\""))
        {
            return true;
        }
    }

    if let Some(script) = &job.script {
        if line.contains(script.to_string_lossy().as_ref()) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{
        build_queue_pressure_output, build_submit_job, build_triage_job_output,
        build_update_request, expand_submit_job_requests, preview_submit_jobs_output,
        preview_update_job_output, resolve_list_jobs_page, resolve_log_slice, serialize_list_job,
        slice_text, GetJobLogRequest, GflowMcpServer, ListJobsDetailInput, ListJobsOrderInput,
        ListJobsOutput, ListJobsRequest, SubmitJobRequest, TextSlice, UpdateJobToolRequest,
        DEFAULT_MCP_LIST_JOBS_LIMIT,
    };
    use compact_str::CompactString;
    use gflow::client::UpdateJobRequest;
    use gflow::core::gpu_allocation::GpuAllocationStrategy;
    use gflow::core::info::{GpuInfo, SchedulerInfo};
    use gflow::core::job::{JobBuilder, JobState, JobStateReason};
    use gflow::core::reservation::{GpuReservation, GpuSpec, ReservationStatus};
    use schemars::schema_for;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::time::{Duration, SystemTime};

    #[test]
    fn tool_schemas_are_exposed_for_object_outputs() {
        let server = GflowMcpServer::new(None);
        let tools = server.tool_router.list_all();

        for tool_name in [
            "get_info",
            "get_health",
            "get_job",
            "get_job_log",
            "get_stats",
            "get_queue_pressure",
            "cancel_job",
            "hold_job",
            "release_job",
            "preview_submit_jobs",
            "submit_jobs",
            "preview_update_job",
            "update_job",
            "triage_job",
            "redo_job",
        ] {
            let tool = tools
                .iter()
                .find(|tool| tool.name == tool_name)
                .unwrap_or_else(|| panic!("missing tool: {tool_name}"));
            assert!(
                tool.output_schema.is_some(),
                "expected output schema for {tool_name}"
            );
        }
    }

    #[test]
    fn submit_job_validation_rejects_shared_jobs_without_gpu_memory_limit() {
        let err = build_submit_job(SubmitJobRequest {
            command: Some("echo hello".to_string()),
            script: None,
            gpus: Some(1),
            conda_env: None,
            run_dir: None,
            priority: None,
            depends_on: None,
            depends_on_ids: None,
            dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            shared: Some(true),
            gpu_memory_limit_mb: None,
            time_limit_secs: None,
            memory_limit_mb: None,
            submitted_by: None,
            param: None,
            parameters: None,
            run_name: None,
            project: None,
            max_concurrent: None,
            max_retries: None,
            auto_close_tmux: None,
            notify_email: None,
            notify_on: None,
        })
        .unwrap_err();

        assert_eq!(
            err,
            "submit_job requires 'gpu_memory_limit_mb' when 'shared' is true"
        );
    }

    #[test]
    fn preview_submit_jobs_expands_without_assigning_job_ids() {
        let output = preview_submit_jobs_output(
            vec![SubmitJobRequest {
                command: Some("echo {lr}".to_string()),
                script: None,
                gpus: Some(1),
                conda_env: None,
                run_dir: None,
                priority: None,
                depends_on: None,
                depends_on_ids: None,
                dependency_mode: None,
                auto_cancel_on_dependency_failure: None,
                shared: None,
                gpu_memory_limit_mb: None,
                time_limit_secs: None,
                memory_limit_mb: None,
                submitted_by: Some("alice".to_string()),
                param: Some(vec!["lr=0.1,0.2".to_string()]),
                parameters: None,
                run_name: None,
                project: None,
                max_concurrent: None,
                max_retries: None,
                auto_close_tmux: None,
                notify_email: None,
                notify_on: None,
            }],
            1,
        );

        assert!(output.dry_run);
        assert!(output.valid);
        assert_eq!(output.input_count, 1);
        assert_eq!(output.expanded_count, 2);
        assert_eq!(output.jobs.len(), 2);
        for result in output.jobs {
            assert!(result.ok);
            assert_eq!(result.input_index, 0);
            assert_eq!(result.job.unwrap()["id"], 0);
        }
    }

    #[test]
    fn preview_update_job_reports_before_after_without_mutating_original() {
        let mut job = JobBuilder::new()
            .command("echo old")
            .submitted_by("alice")
            .gpus(1)
            .priority(10)
            .build();
        job.id = 7;
        job.state = JobState::Queued;

        let request = UpdateJobRequest {
            command: Some("echo new".to_string()),
            gpus: Some(2),
            priority: Some(5),
            memory_limit_mb: Some(Some(4096)),
            ..Default::default()
        };

        let output = preview_update_job_output(job, request);

        assert!(output.dry_run);
        assert!(output.ok);
        assert_eq!(output.job_id, 7);
        assert_eq!(
            output.updated_fields,
            vec!["command", "gpus", "priority", "memory_limit_mb"]
        );

        let before = output.before.expect("before should be present");
        let after = output.after.expect("after should be present");
        assert_eq!(before["command"], "echo old");
        assert_eq!(before["gpus"], 1);
        assert_eq!(before["priority"], 10);
        assert_eq!(after["command"], "echo new");
        assert_eq!(after["gpus"], 2);
        assert_eq!(after["priority"], 5);
        assert_eq!(after["memory_limit_mb"], 4096);
    }

    #[test]
    fn triage_job_includes_log_based_retry_hints_and_exit_status_note() {
        let mut job = JobBuilder::new()
            .command("python train.py")
            .submitted_by("alice")
            .gpus(1)
            .max_retries(2)
            .build();
        job.id = 11;
        job.state = JobState::Failed;
        job.started_at = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(10));
        job.finished_at = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(25));

        let output = build_triage_job_output(
            job,
            Some("/tmp/gflow-11.log".to_string()),
            Some("RuntimeError: CUDA OOM while allocating tensor".to_string()),
        )
        .expect("triage output should build");

        assert_eq!(output.job_id, 11);
        assert_eq!(output.state, "Failed");
        assert_eq!(output.runtime_secs, Some(15.0));
        assert_eq!(output.exit_status, None);
        assert!(output
            .exit_status_note
            .contains("not the process exit code"));
        assert_eq!(output.log_path.as_deref(), Some("/tmp/gflow-11.log"));
        assert!(output
            .retry_hints
            .iter()
            .any(|hint| hint.contains("log suggests OOM")));
        assert!(output
            .retry_hints
            .iter()
            .any(|hint| hint.contains("max_retries=2")));
    }

    #[test]
    fn queue_pressure_summarizes_gpu_pressure_and_groups() {
        let info = SchedulerInfo {
            gpus: vec![
                GpuInfo {
                    uuid: "gpu-0".to_string(),
                    index: 0,
                    available: false,
                    reason: Some("running gflow job".to_string()),
                },
                GpuInfo {
                    uuid: "gpu-1".to_string(),
                    index: 1,
                    available: true,
                    reason: None,
                },
            ],
            allowed_gpu_indices: None,
            gpu_allocation_strategy: GpuAllocationStrategy::Sequential,
        };

        let mut running = JobBuilder::new()
            .command("python train.py")
            .submitted_by("alice")
            .project(Some("vision".to_string()))
            .gpus(1)
            .build();
        running.id = 1;
        running.state = JobState::Running;
        running.gpu_ids = Some(vec![0].into());

        let mut queued = JobBuilder::new()
            .command("python eval.py")
            .submitted_by("alice")
            .project(Some("vision".to_string()))
            .gpus(2)
            .build();
        queued.id = 2;
        queued.state = JobState::Queued;
        queued.reason = Some(Box::new(JobStateReason::WaitingForGpu));

        let mut held = JobBuilder::new()
            .command("echo held")
            .submitted_by("bob")
            .gpus(0)
            .build();
        held.id = 3;
        held.state = JobState::Hold;

        let reservation = GpuReservation {
            id: 1,
            user: CompactString::from("alice"),
            gpu_spec: GpuSpec::Count(1),
            start_time: SystemTime::now() - Duration::from_secs(60),
            duration: Duration::from_secs(3600),
            status: ReservationStatus::Active,
            created_at: SystemTime::now() - Duration::from_secs(120),
            cancelled_at: None,
        };

        let output =
            build_queue_pressure_output(info, vec![running, queued, held], vec![reservation]);

        assert_eq!(output.total_gpus, 2);
        assert_eq!(output.available_gpus, vec![1]);
        assert_eq!(output.unavailable_gpus.len(), 1);
        assert_eq!(output.running_jobs, 1);
        assert_eq!(output.queued_jobs, 1);
        assert_eq!(output.held_jobs, 1);
        assert_eq!(output.queued_requested_gpus, 2);
        assert_eq!(output.running_allocated_gpus, 1);
        assert_eq!(output.blocked_reasons.get("Resources"), Some(&1));
        assert_eq!(output.blocked_reasons.get("JobHeldUser"), Some(&1));
        assert_eq!(output.reservations_total, 1);
        assert_eq!(output.reservations_active, 1);
        assert_eq!(output.users[0].name, "alice");
        assert_eq!(output.users[0].queued, 1);
        assert_eq!(output.users[0].running, 1);
        assert_eq!(output.projects[0].name, "vision");
    }

    #[test]
    fn submit_job_maps_notifications_from_mcp_fields() {
        let job = build_submit_job(SubmitJobRequest {
            command: Some("echo hello".to_string()),
            script: None,
            gpus: Some(0),
            conda_env: None,
            run_dir: None,
            priority: None,
            depends_on: None,
            depends_on_ids: None,
            dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            shared: None,
            gpu_memory_limit_mb: None,
            time_limit_secs: None,
            memory_limit_mb: None,
            submitted_by: None,
            param: None,
            parameters: None,
            run_name: None,
            project: None,
            max_concurrent: None,
            max_retries: None,
            auto_close_tmux: None,
            notify_email: Some(vec!["alice@example.com".to_string()]),
            notify_on: Some(vec!["JOB_FAILED".to_string(), "job_timeout".to_string()]),
        })
        .expect("submit job should build");

        assert_eq!(job.notifications.emails.len(), 1);
        assert_eq!(job.notifications.emails[0].as_str(), "alice@example.com");
        assert_eq!(
            job.notifications
                .events
                .iter()
                .map(|event| event.as_str())
                .collect::<Vec<_>>(),
            vec!["job_failed", "job_timeout"]
        );
    }

    #[test]
    fn update_job_maps_notifications_from_mcp_fields() {
        let request = build_update_request(UpdateJobToolRequest {
            job_id: 7,
            command: None,
            script: None,
            gpus: None,
            conda_env: None,
            clear_conda_env: None,
            priority: None,
            parameters: None,
            time_limit_secs: None,
            clear_time_limit: None,
            memory_limit_mb: None,
            clear_memory_limit: None,
            gpu_memory_limit_mb: None,
            clear_gpu_memory_limit: None,
            depends_on_ids: None,
            dependency_mode: None,
            clear_dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            max_concurrent: None,
            clear_max_concurrent: None,
            max_retries: None,
            clear_max_retries: None,
            notify_email: Some(vec!["alice@example.com".to_string()]),
            notify_on: Some(vec!["job_failed".to_string()]),
        })
        .expect("update request should build");

        let notifications = request
            .notifications
            .expect("notifications should be present");
        assert_eq!(notifications.emails.len(), 1);
        assert_eq!(notifications.emails[0].as_str(), "alice@example.com");
        assert_eq!(
            notifications
                .events
                .iter()
                .map(|event| event.as_str())
                .collect::<Vec<_>>(),
            vec!["job_failed"]
        );
    }

    #[test]
    fn update_job_rejects_notify_on_without_notify_email() {
        let err = build_update_request(UpdateJobToolRequest {
            job_id: 7,
            command: None,
            script: None,
            gpus: None,
            conda_env: None,
            clear_conda_env: None,
            priority: None,
            parameters: None,
            time_limit_secs: None,
            clear_time_limit: None,
            memory_limit_mb: None,
            clear_memory_limit: None,
            gpu_memory_limit_mb: None,
            clear_gpu_memory_limit: None,
            depends_on_ids: None,
            dependency_mode: None,
            clear_dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            max_concurrent: None,
            clear_max_concurrent: None,
            max_retries: None,
            clear_max_retries: None,
            notify_email: None,
            notify_on: Some(vec!["job_failed".to_string()]),
        })
        .unwrap_err();

        assert_eq!(
            err,
            "update_job requires 'notify_email' when 'notify_on' is set"
        );
    }

    #[test]
    fn list_outputs_expose_object_schemas() {
        let server = GflowMcpServer::new(None);
        let tools = server.tool_router.list_all();

        for tool_name in ["list_jobs", "list_reservations"] {
            let tool = tools
                .iter()
                .find(|tool| tool.name == tool_name)
                .unwrap_or_else(|| panic!("missing tool: {tool_name}"));
            assert!(
                tool.output_schema.is_some(),
                "expected output schema for {tool_name}"
            );
        }
    }

    #[test]
    fn list_jobs_defaults_to_recent_first_paging() {
        let resolved = resolve_list_jobs_page(&ListJobsRequest {
            state: None,
            user: None,
            limit: None,
            offset: None,
            created_after: None,
            order: None,
            detail: None,
        });

        assert_eq!(resolved.limit, DEFAULT_MCP_LIST_JOBS_LIMIT);
        assert_eq!(resolved.offset, 0);
        assert_eq!(resolved.order, ListJobsOrderInput::Desc);
        assert_eq!(resolved.detail, ListJobsDetailInput::Summary);
        assert_eq!(resolved.query_limit, DEFAULT_MCP_LIST_JOBS_LIMIT + 1);
    }

    #[test]
    fn list_jobs_honors_explicit_paging_request() {
        let resolved = resolve_list_jobs_page(&ListJobsRequest {
            state: Some("Running".to_string()),
            user: Some("alice".to_string()),
            limit: Some(12),
            offset: Some(24),
            created_after: Some(1_700_000_000),
            order: Some(ListJobsOrderInput::Asc),
            detail: Some(ListJobsDetailInput::Full),
        });

        assert_eq!(resolved.limit, 12);
        assert_eq!(resolved.offset, 24);
        assert_eq!(resolved.order, ListJobsOrderInput::Asc);
        assert_eq!(resolved.detail, ListJobsDetailInput::Full);
        assert_eq!(resolved.query_limit, 13);
    }

    #[test]
    fn list_jobs_output_schema_includes_pagination_fields() {
        let schema = schema_for!(ListJobsOutput);
        let schema_json = serde_json::to_value(&schema).expect("schema should serialize");
        let properties = schema_json
            .get("properties")
            .and_then(Value::as_object)
            .expect("schema should expose properties");

        for field in [
            "jobs",
            "count",
            "detail",
            "limit",
            "offset",
            "has_more",
            "next_offset",
        ] {
            assert!(
                properties.contains_key(field),
                "missing list_jobs output field in schema: {field}"
            );
        }
    }

    #[test]
    fn get_job_log_supports_first_lines() {
        let slice = resolve_log_slice(&GetJobLogRequest {
            job_id: 7,
            first_lines: Some(10),
            last_lines: None,
            max_bytes: None,
        })
        .expect("first_lines should resolve");

        assert_eq!(slice, TextSlice::First(10));
    }

    #[test]
    fn get_job_log_accepts_tail_lines_as_deprecated_alias() {
        let params: GetJobLogRequest = serde_json::from_value(serde_json::json!({
            "job_id": 7,
            "tail_lines": 25
        }))
        .expect("tail_lines alias should deserialize");
        let slice = resolve_log_slice(&params).expect("tail_lines alias should resolve");

        assert_eq!(slice, TextSlice::Last(25));
    }

    #[test]
    fn get_job_log_rejects_conflicting_line_slice_options() {
        let err = resolve_log_slice(&GetJobLogRequest {
            job_id: 7,
            first_lines: Some(10),
            last_lines: Some(20),
            max_bytes: None,
        })
        .expect_err("conflicting options should fail");

        assert!(err
            .to_string()
            .contains("only one of first_lines or last_lines"));
    }

    #[test]
    fn get_job_log_schema_hides_deprecated_tail_lines_field() {
        let schema = schema_for!(GetJobLogRequest);
        let schema_json = serde_json::to_value(&schema).expect("schema should serialize");
        let properties = schema_json
            .get("properties")
            .and_then(Value::as_object)
            .expect("schema should expose properties");

        assert!(properties.contains_key("first_lines"));
        assert!(properties.contains_key("last_lines"));
        assert!(!properties.contains_key("tail_lines"));
    }

    #[test]
    fn slice_text_can_take_first_lines() {
        let output = slice_text("a\nb\nc\nd".to_string(), TextSlice::First(2), None);
        assert_eq!(output, "a\nb");
    }

    #[test]
    fn slice_text_can_take_last_lines() {
        let output = slice_text("a\nb\nc\nd".to_string(), TextSlice::Last(2), None);
        assert_eq!(output, "c\nd");
    }

    #[test]
    fn list_jobs_summary_is_compact() {
        let mut job = JobBuilder::new()
            .command("python train.py --epochs 100")
            .submitted_by("alice")
            .run_name(Some("exp-42".to_string()))
            .project(Some("vision".to_string()))
            .gpus(2)
            .build();
        job.id = 42;
        job.state = JobState::Running;
        job.reason = Some(Box::new(JobStateReason::WaitingForResources));

        let value = serialize_list_job(job, ListJobsDetailInput::Summary);
        let object = value.as_object().expect("summary should be an object");

        for field in [
            "id",
            "name",
            "state",
            "reason",
            "gpus",
            "gpu_ids",
            "user",
            "project",
            "submitted",
            "started",
            "finished",
        ] {
            assert!(object.contains_key(field), "missing summary field: {field}");
        }

        for field in [
            "command",
            "script",
            "conda_env",
            "run_dir",
            "parameters",
            "depends_on",
            "depends_on_ids",
            "memory_limit_mb",
            "time_limit",
        ] {
            assert!(
                !object.contains_key(field),
                "summary should omit verbose field: {field}"
            );
        }
    }

    #[test]
    fn submit_jobs_expand_cli_style_param_combinations() {
        let expanded = expand_submit_job_requests(vec![SubmitJobRequest {
            command: Some("python train.py --lr {lr} --batch-size {bs}".to_string()),
            script: None,
            gpus: Some(0),
            conda_env: None,
            run_dir: None,
            priority: None,
            depends_on: None,
            depends_on_ids: None,
            dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            shared: None,
            gpu_memory_limit_mb: None,
            time_limit_secs: None,
            memory_limit_mb: None,
            submitted_by: None,
            param: Some(vec!["lr=0.001,0.01".to_string(), "bs=32,64".to_string()]),
            parameters: Some(HashMap::from([("seed".to_string(), "123".to_string())])),
            run_name: None,
            project: None,
            max_concurrent: None,
            max_retries: None,
            auto_close_tmux: None,
            notify_email: None,
            notify_on: None,
        }])
        .unwrap();

        assert_eq!(expanded.len(), 4);
        assert!(expanded.iter().all(|(index, _)| *index == 0));
        assert!(expanded.iter().all(|(_, job)| job.param.is_none()));
        assert_eq!(
            expanded[0]
                .1
                .parameters
                .as_ref()
                .and_then(|params| params.get("seed"))
                .map(String::as_str),
            Some("123")
        );
        assert_eq!(
            expanded[0]
                .1
                .parameters
                .as_ref()
                .and_then(|params| params.get("lr"))
                .map(String::as_str),
            Some("0.001")
        );
        assert_eq!(
            expanded[3]
                .1
                .parameters
                .as_ref()
                .and_then(|params| params.get("lr"))
                .map(String::as_str),
            Some("0.01")
        );
        assert_eq!(
            expanded[3]
                .1
                .parameters
                .as_ref()
                .and_then(|params| params.get("bs"))
                .map(String::as_str),
            Some("64")
        );
    }

    #[test]
    fn submit_jobs_reject_duplicate_keys_between_parameters_and_param() {
        let err = expand_submit_job_requests(vec![SubmitJobRequest {
            command: Some("echo {lr}".to_string()),
            script: None,
            gpus: None,
            conda_env: None,
            run_dir: None,
            priority: None,
            depends_on: None,
            depends_on_ids: None,
            dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            shared: None,
            gpu_memory_limit_mb: None,
            time_limit_secs: None,
            memory_limit_mb: None,
            submitted_by: None,
            param: Some(vec!["lr=0.001,0.01".to_string()]),
            parameters: Some(HashMap::from([("lr".to_string(), "0.1".to_string())])),
            run_name: None,
            project: None,
            max_concurrent: None,
            max_retries: None,
            auto_close_tmux: None,
            notify_email: None,
            notify_on: None,
        }])
        .unwrap_err();

        assert_eq!(
            err,
            "submit_job cannot use the same key in both 'parameters' and 'param': lr"
        );
    }
}
