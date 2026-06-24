mod helpers;
mod list_jobs;
mod log;
mod queue_pressure;
mod schemas;
mod submit;
#[cfg(test)]
mod tests;
mod triage;
mod update;

use anyhow::Result;
use clap_verbosity_flag::Verbosity;
use gflow::client::UpdateJobResponse;
use gflow::Client;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
    ServiceExt,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use helpers::*;
use list_jobs::*;
use log::*;
use queue_pressure::*;
use schemas::*;
use submit::*;
use triage::*;
use update::*;

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
