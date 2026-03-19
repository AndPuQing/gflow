use anyhow::Result;
use clap_verbosity_flag::Verbosity;
use gflow::client::{UpdateJobRequest, UpdateJobResponse};
use gflow::core::job::{DependencyMode, Job, JobBuilder};
use gflow::Client;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::stdio,
    ServiceExt,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListJobsRequest {
    /// Comma-separated job states, for example `Running,Finished`.
    pub state: Option<String>,
    /// Filter by submitting user.
    pub user: Option<String>,
    /// Defaults to 100 when omitted.
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Unix timestamp in seconds.
    pub created_after: Option<i64>,
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

#[derive(Debug, Deserialize, JsonSchema)]
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
    pub parameters: Option<HashMap<String, String>>,
    pub run_name: Option<String>,
    pub project: Option<String>,
    pub max_concurrent: Option<usize>,
    pub auto_close_tmux: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubmitJobsBatchRequest {
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
pub struct JobSubmitOutput {
    pub id: u32,
    pub run_name: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct BatchJobSubmitOutput {
    pub jobs: Vec<JobSubmitOutput>,
    pub count: usize,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct UpdateJobOutputSchema {
    pub job: ArbitraryObjectSchema,
    pub updated_fields: Vec<String>,
}

#[derive(Clone)]
struct GflowMcpServer {
    config_path: Option<PathBuf>,
    tool_router: ToolRouter<Self>,
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

    #[tool(description = "List jobs from the local gflow daemon.")]
    async fn list_jobs(
        &self,
        Parameters(params): Parameters<ListJobsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let jobs = client
            .list_jobs_with_query(
                params.state,
                params.user,
                Some(params.limit.unwrap_or(100)),
                params.offset,
                params.created_after,
            )
            .await
            .map_err(stringify_error)?;
        structured_response(jobs)
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

    #[tool(description = "List GPU reservations from the local gflow daemon.")]
    async fn list_reservations(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let reservations = client
            .list_reservations(None, None, false)
            .await
            .map_err(stringify_error)?;
        structured_response(reservations)
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
        description = "Submit a job to the local gflow daemon using a simplified schema.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<JobSubmitOutput>()
    )]
    async fn submit_job(
        &self,
        Parameters(params): Parameters<SubmitJobRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let job = build_submit_job(params).map_err(|err| stringify_error(anyhow::anyhow!(err)))?;
        let response = client.add_job(job).await.map_err(stringify_error)?;
        structured_response(response)
    }

    #[tool(
        description = "Submit multiple jobs to the local gflow daemon using the same simplified schema as submit_job.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<BatchJobSubmitOutput>()
    )]
    async fn submit_jobs_batch(
        &self,
        Parameters(params): Parameters<SubmitJobsBatchRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = self.client().map_err(stringify_error)?;
        let jobs =
            build_submit_jobs_batch(params).map_err(|err| stringify_error(anyhow::anyhow!(err)))?;
        let responses = client.add_jobs(jobs).await.map_err(stringify_error)?;
        structured_response(json!({
            "jobs": responses,
            "count": responses.len(),
        }))
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

fn build_submit_jobs_batch(params: SubmitJobsBatchRequest) -> Result<Vec<Job>, String> {
    if params.jobs.is_empty() {
        return Err("submit_jobs_batch requires at least one job".to_string());
    }
    if params.jobs.len() > 1000 {
        return Err("submit_jobs_batch accepts at most 1000 jobs".to_string());
    }

    params.jobs.into_iter().map(build_submit_job).collect()
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
    })
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
        build_submit_jobs_batch, GflowMcpServer, SubmitJobRequest, SubmitJobsBatchRequest,
    };

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
            "submit_job",
            "submit_jobs_batch",
            "update_job",
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
    fn submit_jobs_batch_rejects_empty_batches() {
        let err = build_submit_jobs_batch(SubmitJobsBatchRequest { jobs: vec![] }).unwrap_err();
        assert_eq!(err, "submit_jobs_batch requires at least one job");
    }

    #[test]
    fn submit_jobs_batch_reuses_single_job_validation() {
        let err = build_submit_jobs_batch(SubmitJobsBatchRequest {
            jobs: vec![SubmitJobRequest {
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
                parameters: None,
                run_name: None,
                project: None,
                max_concurrent: None,
                auto_close_tmux: None,
            }],
        })
        .unwrap_err();

        assert_eq!(
            err,
            "submit_job requires 'gpu_memory_limit_mb' when 'shared' is true"
        );
    }

    #[test]
    fn array_outputs_remain_schema_less_for_compatibility() {
        let server = GflowMcpServer::new(None);
        let tools = server.tool_router.list_all();

        for tool_name in ["list_jobs", "list_reservations"] {
            let tool = tools
                .iter()
                .find(|tool| tool.name == tool_name)
                .unwrap_or_else(|| panic!("missing tool: {tool_name}"));
            assert!(
                tool.output_schema.is_none(),
                "expected no output schema for {tool_name}"
            );
        }
    }
}
