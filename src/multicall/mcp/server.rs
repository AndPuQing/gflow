use anyhow::Result;
use clap_verbosity_flag::Verbosity;
use gflow::client::{UpdateJobRequest, UpdateJobResponse};
use gflow::core::job::{DependencyMode, Job, JobBuilder, JobNotifications};
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
use std::collections::HashMap;
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
    /// Return only the last N lines.
    pub tail_lines: Option<usize>,
    /// Truncate to the last N bytes.
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetStatsRequest {
    pub user: Option<String>,
    /// Unix timestamp in seconds.
    pub since: Option<i64>,
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
        let cleaned = slice_text(
            clean_terminal_output(&raw),
            params.tail_lines,
            params.max_bytes,
        );
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
        description = "Cancel a job through the local gflow daemon.",
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
        description = "Put a queued job on hold through the local gflow daemon.",
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
        description = "Release a held job through the local gflow daemon.",
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
        description = "Submit one or more jobs to the local gflow daemon using a simplified schema. Jobs are attempted sequentially and each result reports success or failure without aborting the whole request.",
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
        description = "Update mutable job parameters on the local gflow daemon.",
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
        description = "Resubmit a finished job with the same or overridden parameters, optionally cascading to dependency-cancelled child jobs.",
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
            "Local-first gflow MCP server. Prefer read tools before mutating scheduler state."
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

fn slice_text(text: String, tail_lines: Option<usize>, max_bytes: Option<usize>) -> String {
    let mut output = text;

    if let Some(tail_lines) = tail_lines {
        let lines: Vec<_> = output.lines().collect();
        output = lines
            .into_iter()
            .rev()
            .take(tail_lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
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
        build_submit_job, build_update_request, expand_submit_job_requests, resolve_list_jobs_page,
        serialize_list_job, GflowMcpServer, ListJobsDetailInput, ListJobsOrderInput,
        ListJobsOutput, ListJobsRequest, SubmitJobRequest, UpdateJobToolRequest,
        DEFAULT_MCP_LIST_JOBS_LIMIT,
    };
    use gflow::core::job::{JobBuilder, JobState, JobStateReason};
    use schemars::schema_for;
    use serde_json::Value;
    use std::collections::HashMap;

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
            "cancel_job",
            "hold_job",
            "release_job",
            "submit_jobs",
            "update_job",
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
