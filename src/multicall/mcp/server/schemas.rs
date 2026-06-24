use gflow::core::job::DependencyMode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

pub(super) const DEFAULT_MCP_LIST_JOBS_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
pub(super) enum DependencyModeInput {
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
pub(super) enum ListJobsOrderInput {
    Asc,
    Desc,
}

impl ListJobsOrderInput {
    pub(super) fn as_query_value(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum ListJobsDetailInput {
    Summary,
    Full,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct ListJobsRequest {
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
pub(super) struct JobIdRequest {
    pub job_id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct GetJobLogRequest {
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
pub(super) struct GetStatsRequest {
    pub user: Option<String>,
    /// Unix timestamp in seconds.
    pub since: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct TriageJobRequest {
    pub job_id: u32,
    /// Defaults to the last 80 lines.
    #[serde(alias = "tail_lines")]
    pub last_lines: Option<usize>,
    /// Defaults to the last 20000 bytes after slicing.
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(super) struct SubmitJobRequest {
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
pub(super) struct SubmitJobsRequest {
    pub jobs: Vec<SubmitJobRequest>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct UpdateJobToolRequest {
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
pub(super) struct RedoJobRequest {
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
pub(super) struct ArbitraryObjectSchema {
    #[schemars(flatten)]
    pub fields: HashMap<String, Value>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct HealthOutput {
    pub status: u16,
    pub ok: bool,
    pub pid: Option<u32>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct GpuInfoOutput {
    pub uuid: String,
    pub index: u32,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct SchedulerInfoOutput {
    pub gpus: Vec<GpuInfoOutput>,
    pub allowed_gpu_indices: Option<Vec<u32>>,
    pub gpu_allocation_strategy: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct ListJobsOutput {
    pub jobs: Vec<Value>,
    pub count: usize,
    pub detail: ListJobsDetailInput,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct ListReservationsOutput {
    pub reservations: Vec<Value>,
    pub count: usize,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct PreviewSubmitJobOutput {
    pub dry_run: bool,
    pub valid: bool,
    pub input_count: usize,
    pub expanded_count: usize,
    pub jobs: Vec<PreviewSubmitJobResultOutput>,
    pub warnings: Vec<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct PreviewSubmitJobResultOutput {
    pub input_index: usize,
    pub expanded_index: usize,
    pub ok: bool,
    pub job: Option<Value>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct PreviewUpdateJobOutput {
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
pub(super) struct TriageJobOutput {
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
pub(super) struct QueuePressureOutput {
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
pub(super) struct GpuAvailabilityOutput {
    pub index: u32,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct QueuePressureGroupOutput {
    pub name: String,
    pub queued: usize,
    pub running: usize,
    pub requested_gpus: u32,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct JobLogOutput {
    pub job_id: u32,
    pub log_path: String,
    pub text: String,
    pub program_output: Option<String>,
    pub full_text: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct TopJobOutput {
    pub id: u32,
    pub name: Option<String>,
    pub runtime_secs: f64,
    pub gpus: u32,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct UsageStatsOutput {
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
pub(super) struct JobActionOutput {
    pub job_id: u32,
    pub cancelled: Option<bool>,
    pub held: Option<bool>,
    pub released: Option<bool>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct SubmitJobResultOutput {
    pub index: usize,
    pub ok: bool,
    pub job_id: Option<u32>,
    pub run_name: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct SubmitJobsOutput {
    pub results: Vec<SubmitJobResultOutput>,
    pub submitted: usize,
    pub failed: usize,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct UpdateJobOutputSchema {
    pub job: ArbitraryObjectSchema,
    pub updated_fields: Vec<String>,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct RedoCascadeJobOutput {
    pub original_job_id: u32,
    pub new_job_id: u32,
    pub run_name: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub(super) struct RedoJobOutput {
    pub original_job_id: u32,
    pub new_job_id: u32,
    pub run_name: String,
    pub cascaded_jobs: Vec<RedoCascadeJobOutput>,
    pub cascaded_count: usize,
}
