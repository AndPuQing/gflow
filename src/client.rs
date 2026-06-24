use crate::core::info::{IgnoredGpuProcess, SchedulerInfo};
use crate::core::job::{DependencyMode, Job, JobNotifications};
use anyhow::{anyhow, Context};
use reqwest::{Client as ReqwestClient, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Checks if an error is a connection error and returns a user-friendly message
fn connection_error_context(err: reqwest::Error) -> anyhow::Error {
    if err.is_connect() {
        anyhow!(
            "Could not connect to gflowd server. Is the server running?\n\
             Hint: Start the server with 'gflowd up'"
        )
    } else {
        err.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSubmitResponse {
    pub id: u32,
    pub run_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedJobsResponse {
    pub jobs: Vec<Job>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateJobRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpus: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conda_env: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_limit: Option<Option<Duration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit_mb: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_limit_mb: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on_ids: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_mode: Option<Option<DependencyMode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_cancel_on_dependency_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<Option<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications: Option<JobNotifications>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobResponse {
    pub job: Job,
    pub updated_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopJob {
    pub id: u32,
    pub name: Option<String>,
    pub runtime_secs: f64,
    pub gpus: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
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
    pub top_jobs: Vec<TopJob>,
}

#[derive(Debug, Clone)]
pub struct Client {
    client: ReqwestClient,
    base_url: String,
}

impl Client {
    pub fn build(config: &crate::config::Config) -> anyhow::Result<Self> {
        crate::tls::ensure_rustls_provider_installed();
        let host = &config.daemon.host;
        let port = config.daemon.port;
        let base_url = format!("http://{host}:{port}");
        let client = ReqwestClient::new();
        Ok(Self { client, base_url })
    }

    /// Helper to extract error message from response
    async fn extract_error_message(response: reqwest::Response) -> String {
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("Unknown error"));

        // Try to parse as JSON with error field
        if let Ok(json_error) = serde_json::from_str::<serde_json::Value>(&error_body) {
            if let Some(error_msg) = json_error.get("error").and_then(|e| e.as_str()) {
                return error_msg.to_string();
            }
        }

        error_body
    }

    async fn post_expect_success(&self, path: String, action: &str) -> anyhow::Result<()> {
        let response = self
            .client
            .post(path)
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_msg = Self::extract_error_message(response).await;
            let detail = if error_msg.trim().is_empty() {
                status.to_string()
            } else {
                format!("{status}: {error_msg}")
            };
            return Err(anyhow!("Failed to {action}: {detail}"));
        }

        Ok(())
    }

    /// List jobs with optional query parameters.
    ///
    /// If no parameters are provided, returns jobs from memory (active jobs only).
    /// If parameters are provided, queries from database with pagination support.
    pub async fn list_jobs(&self) -> anyhow::Result<Vec<Job>> {
        let jobs = self
            .client
            .get(format!("{}/jobs", self.base_url))
            .send()
            .await
            .map_err(connection_error_context)?
            .json::<Vec<Job>>()
            .await
            .context("Failed to parse jobs from response")?;
        Ok(jobs)
    }

    /// List jobs with query parameters for database queries.
    ///
    /// This method queries from the database and supports:
    /// - State filtering (e.g., "Running,Finished")
    /// - User filtering (e.g., "user1,user2")
    /// - Pagination (limit and offset)
    /// - Time filtering (created_after timestamp)
    /// - Ordering (`asc` or `desc`)
    ///
    /// Returns all matching jobs from the database, not just in-memory jobs.
    pub async fn list_jobs_with_query(
        &self,
        states: Option<String>,
        user: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
        created_after: Option<i64>,
        order: Option<String>,
    ) -> anyhow::Result<Vec<Job>> {
        let mut request = self.client.get(format!("{}/jobs", self.base_url));

        // Add query parameters if provided
        let mut params = vec![];
        if let Some(s) = states {
            params.push(("state", s));
        }
        if let Some(u) = user {
            params.push(("user", u));
        }
        if let Some(l) = limit {
            params.push(("limit", l.to_string()));
        }
        if let Some(o) = offset {
            params.push(("offset", o.to_string()));
        }
        if let Some(t) = created_after {
            params.push(("created_after", t.to_string()));
        }
        if let Some(order) = order {
            params.push(("order", order));
        }

        if !params.is_empty() {
            request = request.query(&params);
        }

        let response = request.send().await.map_err(connection_error_context)?;

        // Handle both direct Vec<Job> and paginated response
        let response_text = response.text().await?;

        // Try to parse as PaginatedJobsResponse first
        if let Ok(paginated) = serde_json::from_str::<PaginatedJobsResponse>(&response_text) {
            Ok(paginated.jobs)
        } else {
            // Fall back to direct Vec<Job> for backward compatibility
            serde_json::from_str::<Vec<Job>>(&response_text)
                .context("Failed to parse jobs from response")
        }
    }

    pub async fn get_job(&self, job_id: u32) -> anyhow::Result<Option<Job>> {
        tracing::debug!("Getting job {job_id}");
        let response = self
            .client
            .get(format!("{}/jobs/{}", self.base_url, job_id))
            .send()
            .await
            .map_err(connection_error_context)?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let job = response
            .json::<Job>()
            .await
            .context("Failed to parse job from response")?;
        Ok(Some(job))
    }

    pub async fn add_job(&self, job: Job) -> anyhow::Result<JobSubmitResponse> {
        tracing::debug!("Adding job: {job:?}");
        let response = self
            .client
            .post(format!("{}/jobs", self.base_url))
            .json(&job)
            .send()
            .await
            .map_err(connection_error_context)?;

        // Check if the response is successful
        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow::anyhow!("Failed to add job: {}", error_msg));
        }

        let job_response: JobSubmitResponse = response
            .json()
            .await
            .context("Failed to parse response json")?;
        Ok(job_response)
    }

    /// Submit multiple jobs in a batch
    pub async fn add_jobs(&self, jobs: Vec<Job>) -> anyhow::Result<Vec<JobSubmitResponse>> {
        if jobs.is_empty() {
            return Ok(Vec::new());
        }

        tracing::debug!("Adding {} jobs in batch", jobs.len());
        let response = self
            .client
            .post(format!("{}/jobs/batch", self.base_url))
            .json(&jobs)
            .send()
            .await
            .map_err(connection_error_context)?;

        // Check if the response is successful
        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow::anyhow!("Failed to add batch jobs: {}", error_msg));
        }

        let job_responses: Vec<JobSubmitResponse> = response
            .json()
            .await
            .context("Failed to parse batch response json")?;
        Ok(job_responses)
    }

    pub async fn finish_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Finishing job {job_id}");
        self.post_expect_success(
            format!("{}/jobs/{}/finish", self.base_url, job_id),
            "finish job",
        )
        .await
    }

    pub async fn fail_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Failing job {job_id}");
        self.post_expect_success(
            format!("{}/jobs/{}/fail", self.base_url, job_id),
            "fail job",
        )
        .await
    }

    pub async fn cancel_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Cancelling job {job_id}");
        self.post_expect_success(
            format!("{}/jobs/{}/cancel", self.base_url, job_id),
            "cancel job",
        )
        .await
    }

    pub async fn hold_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Holding job {job_id}");
        self.post_expect_success(
            format!("{}/jobs/{}/hold", self.base_url, job_id),
            "hold job",
        )
        .await
    }

    pub async fn release_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Releasing job {job_id}");
        self.post_expect_success(
            format!("{}/jobs/{}/release", self.base_url, job_id),
            "release job",
        )
        .await
    }

    pub async fn update_job(
        &self,
        job_id: u32,
        request: UpdateJobRequest,
    ) -> anyhow::Result<UpdateJobResponse> {
        tracing::debug!("Updating job {job_id}");

        let response = self
            .client
            .patch(format!("{}/jobs/{}", self.base_url, job_id))
            .json(&request)
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to update job: {}", error_msg));
        }

        let result: UpdateJobResponse = response
            .json()
            .await
            .context("Failed to parse update job response")?;

        Ok(result)
    }

    pub async fn get_job_log_path(&self, job_id: u32) -> anyhow::Result<Option<String>> {
        tracing::debug!("Getting log path for job {job_id}");
        let response = self
            .client
            .get(format!("{}/jobs/{}/log", self.base_url, job_id))
            .send()
            .await
            .map_err(connection_error_context)?;
        let status = response.status();
        if status == StatusCode::OK {
            response
                .json::<Option<String>>()
                .await
                .context("Failed to parse log path from response")
        } else if status == StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<failed to read body>"));
            Err(anyhow!(
                "Failed to get log path for job {} (status {}): {}",
                job_id,
                status,
                body
            ))
        }
    }

    pub async fn get_stats(
        &self,
        user: Option<&str>,
        since: Option<i64>,
    ) -> anyhow::Result<UsageStats> {
        let mut params = vec![];
        if let Some(u) = user {
            params.push(("user", u.to_string()));
        }
        if let Some(s) = since {
            params.push(("since", s.to_string()));
        }
        let mut request = self.client.get(format!("{}/stats", self.base_url));
        if !params.is_empty() {
            request = request.query(&params);
        }
        let stats = request
            .send()
            .await
            .map_err(connection_error_context)?
            .json::<UsageStats>()
            .await
            .context("Failed to parse stats from response")?;
        Ok(stats)
    }

    pub async fn get_info(&self) -> anyhow::Result<SchedulerInfo> {
        tracing::debug!("Getting scheduler info");
        let info = self
            .client
            .get(format!("{}/info", self.base_url))
            .send()
            .await
            .map_err(connection_error_context)?
            .json::<SchedulerInfo>()
            .await
            .context("Failed to parse info from response")?;
        Ok(info)
    }

    pub async fn get_health(&self) -> anyhow::Result<StatusCode> {
        tracing::debug!("Getting health status");
        let health = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(connection_error_context)?
            .status();
        Ok(health)
    }

    pub async fn get_health_with_pid(&self) -> anyhow::Result<Option<u32>> {
        tracing::debug!("Getting health status with PID");
        let response = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let health_data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse health response")?;

        let pid = health_data
            .get("pid")
            .and_then(|p| p.as_u64())
            .map(|p| p as u32);

        Ok(pid)
    }

    pub async fn resolve_dependency(&self, username: &str, shorthand: &str) -> anyhow::Result<u32> {
        tracing::debug!(
            "Resolving dependency '{}' for user '{}'",
            shorthand,
            username
        );
        let response = self
            .client
            .get(format!("{}/jobs/resolve-dependency", self.base_url))
            .query(&[("username", username), ("shorthand", shorthand)])
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow::anyhow!(
                "Failed to resolve dependency: {}",
                error_msg
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse response json")?;

        let job_id = result
            .get("job_id")
            .and_then(|v| v.as_u64())
            .context("Invalid response format: missing or invalid job_id")?
            as u32;

        Ok(job_id)
    }

    pub async fn set_allowed_gpus(&self, allowed_indices: Option<Vec<u32>>) -> anyhow::Result<()> {
        tracing::debug!("Setting allowed GPU indices: {:?}", allowed_indices);

        let request_body = serde_json::json!({
            "allowed_indices": allowed_indices
        });

        let response = self
            .client
            .post(format!("{}/gpus", self.base_url))
            .json(&request_body)
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to set GPU configuration: {}", error_msg));
        }

        Ok(())
    }

    pub async fn list_ignored_gpu_processes(&self) -> anyhow::Result<Vec<IgnoredGpuProcess>> {
        tracing::debug!("Listing ignored GPU processes");
        let response = self
            .client
            .get(format!("{}/gpu-processes", self.base_url))
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!(
                "Failed to list ignored GPU processes: {}",
                error_msg
            ));
        }

        let processes = response
            .json::<Vec<IgnoredGpuProcess>>()
            .await
            .context("Failed to parse ignored GPU processes from response")?;
        Ok(processes)
    }

    pub async fn ignore_gpu_process(&self, gpu_index: u32, pid: u32) -> anyhow::Result<()> {
        tracing::debug!("Ignoring GPU process pid={} on gpu={}", pid, gpu_index);
        self.post_gpu_process_action("ignore", gpu_index, pid).await
    }

    pub async fn unignore_gpu_process(&self, gpu_index: u32, pid: u32) -> anyhow::Result<()> {
        tracing::debug!(
            "Removing ignored GPU process pid={} on gpu={}",
            pid,
            gpu_index
        );
        self.post_gpu_process_action("unignore", gpu_index, pid)
            .await
    }

    async fn post_gpu_process_action(
        &self,
        action: &str,
        gpu_index: u32,
        pid: u32,
    ) -> anyhow::Result<()> {
        let request_body = serde_json::json!({
            "gpu_index": gpu_index,
            "pid": pid
        });

        let response = self
            .client
            .post(format!("{}/gpu-processes/{}", self.base_url, action))
            .json(&request_body)
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to {} GPU process: {}", action, error_msg));
        }

        Ok(())
    }

    pub async fn set_group_max_concurrency(
        &self,
        group_id: &str,
        max_concurrent: usize,
    ) -> anyhow::Result<usize> {
        tracing::debug!(
            "Setting max_concurrency for group '{}' to {}",
            group_id,
            max_concurrent
        );

        let request_body = serde_json::json!({
            "max_concurrent": max_concurrent
        });

        let response = self
            .client
            .post(format!(
                "{}/groups/{}/max-concurrency",
                self.base_url, group_id
            ))
            .json(&request_body)
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!(
                "Failed to set group max_concurrency: {}",
                error_msg
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse response json")?;

        let updated_jobs = result
            .get("updated_jobs")
            .and_then(|v| v.as_u64())
            .context("Invalid response format: missing or invalid updated_jobs")?
            as usize;

        Ok(updated_jobs)
    }

    /// Create a GPU reservation
    pub async fn create_reservation(
        &self,
        user: String,
        gpu_spec: crate::core::reservation::GpuSpec,
        start_time: std::time::SystemTime,
        duration_secs: u64,
    ) -> anyhow::Result<u32> {
        use crate::core::reservation::GpuSpec;

        let mut request_body = serde_json::json!({
            "user": user,
            "start_time": start_time,
            "duration_secs": duration_secs,
        });

        // Add gpu_count or gpu_indices based on spec type
        match gpu_spec {
            GpuSpec::Count(count) => {
                request_body["gpu_count"] = serde_json::json!(count);
            }
            GpuSpec::Indices(indices) => {
                request_body["gpu_indices"] = serde_json::json!(indices);
            }
        }

        let response = self
            .client
            .post(format!("{}/reservations", self.base_url))
            .json(&request_body)
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to create reservation: {}", error_msg));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse response json")?;

        let reservation_id = result
            .get("reservation_id")
            .and_then(|v| v.as_u64())
            .context("Invalid response format: missing or invalid reservation_id")?
            as u32;

        Ok(reservation_id)
    }

    /// List GPU reservations
    pub async fn list_reservations(
        &self,
        user: Option<String>,
        status: Option<String>,
        active_only: bool,
    ) -> anyhow::Result<Vec<crate::core::reservation::GpuReservation>> {
        let mut url = format!("{}/reservations", self.base_url);
        let mut query_params = Vec::new();

        if let Some(user) = user {
            query_params.push(format!("user={}", user));
        }
        if let Some(status) = status {
            query_params.push(format!("status={}", status));
        }
        if active_only {
            query_params.push("active_only=true".to_string());
        }

        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to list reservations: {}", error_msg));
        }

        let reservations = response
            .json()
            .await
            .context("Failed to parse response json")?;

        Ok(reservations)
    }

    /// Get a specific GPU reservation by ID
    pub async fn get_reservation(
        &self,
        id: u32,
    ) -> anyhow::Result<Option<crate::core::reservation::GpuReservation>> {
        let response = self
            .client
            .get(format!("{}/reservations/{}", self.base_url, id))
            .send()
            .await
            .map_err(connection_error_context)?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to get reservation: {}", error_msg));
        }

        let reservation = response
            .json()
            .await
            .context("Failed to parse response json")?;

        Ok(Some(reservation))
    }

    /// Cancel a GPU reservation
    pub async fn cancel_reservation(&self, id: u32) -> anyhow::Result<()> {
        let response = self
            .client
            .delete(format!("{}/reservations/{}", self.base_url, id))
            .send()
            .await
            .map_err(connection_error_context)?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to cancel reservation: {}", error_msg));
        }

        Ok(())
    }
}

/// Helper function to get a job and print a warning if not found.
/// Returns Ok(Some(job)) if found, Ok(None) if not found (with warning printed).
///
/// This is a convenience function to reduce boilerplate in CLI tools.
pub async fn get_job_or_warn(client: &Client, job_id: u32) -> anyhow::Result<Option<Job>> {
    match client.get_job(job_id).await? {
        Some(job) => Ok(Some(job)),
        None => {
            eprintln!("Error: Job {} not found", job_id);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::core::gpu_allocation::GpuAllocationStrategy;
    use crate::core::job::JobBuilder;
    use crate::core::reservation::GpuSpec;
    use compact_str::CompactString;
    use std::time::SystemTime;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Build a `Client` pointed at the given mock server.
    fn client_for(server: &MockServer) -> Client {
        let mut config = Config::default();
        config.daemon.host = "127.0.0.1".to_string();
        config.daemon.port = server.address().port();
        Client::build(&config).expect("failed to build client")
    }

    /// A minimal job JSON the daemon would return for a queued job.
    fn job_json(id: u32, state: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "state": state,
            "script": null,
            "command": "echo hello",
            "gpus": 1,
            "conda_env": null,
            "run_dir": ".",
            "priority": 10,
            "depends_on": null,
            "depends_on_ids": [],
            "dependency_mode": null,
            "auto_cancel_on_dependency_failure": true,
            "task_id": null,
            "time_limit": null,
            "memory_limit_mb": null,
            "submitted_by": "tester",
            "redone_from": null,
            "auto_close_tmux": false,
            "parameters": {},
            "group_id": null,
            "max_concurrent": null,
            "run_name": null,
            "gpu_ids": null,
            "submitted_at": null,
            "started_at": null,
            "finished_at": null,
            "reason": null
        })
    }

    // ── list_jobs ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_jobs_returns_vec_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(vec![job_json(1, "Queued"), job_json(2, "Running")]),
            )
            .mount(&server)
            .await;

        let client = client_for(&server);
        let jobs = client.list_jobs().await.expect("should list jobs");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, 1);
        assert_eq!(jobs[1].id, 2);
    }

    #[tokio::test]
    async fn list_jobs_propagates_connection_error_with_friendly_message() {
        let mut config = Config::default();
        config.daemon.host = "127.0.0.1".to_string();
        // Use a port that is almost certainly closed.
        config.daemon.port = 1;
        let client = Client::build(&config).expect("failed to build client");

        let err = client.list_jobs().await.unwrap_err();
        assert!(err.to_string().contains("Could not connect to gflowd"));
    }

    // ── list_jobs_with_query ───────────────────────────────────────────────

    #[tokio::test]
    async fn list_jobs_with_query_accepts_paginated_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs"))
            .and(query_param("state", "Running"))
            .and(query_param("limit", "10"))
            .and(query_param("offset", "5"))
            .and(query_param("order", "desc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jobs": [job_json(7, "Running")],
                "total": 42,
                "limit": 10,
                "offset": 5,
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let jobs = client
            .list_jobs_with_query(
                Some("Running".into()),
                None,
                Some(10),
                Some(5),
                None,
                Some("desc".into()),
            )
            .await
            .expect("should list jobs");

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, 7);
    }

    #[tokio::test]
    async fn list_jobs_with_query_falls_back_to_plain_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![job_json(1, "Queued")]))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let jobs = client
            .list_jobs_with_query(None, None, None, None, None, None)
            .await
            .expect("should list jobs");

        assert_eq!(jobs.len(), 1);
    }

    // ── get_job ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_job_returns_some_when_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(job_json(3, "Queued")))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job = client.get_job(3).await.expect("request should succeed");
        assert!(job.is_some());
        assert_eq!(job.unwrap().id, 3);
    }

    #[tokio::test]
    async fn get_job_returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/99"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job = client.get_job(99).await.expect("request should succeed");
        assert!(job.is_none());
    }

    // ── add_job / add_jobs ─────────────────────────────────────────────────

    #[tokio::test]
    async fn add_job_returns_submit_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/jobs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "run_name": "gjob-42"
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job = JobBuilder::new()
            .command("echo hi")
            .submitted_by("tester")
            .build();
        let resp = client.add_job(job).await.expect("should add job");
        assert_eq!(resp.id, 42);
        assert_eq!(resp.run_name, "gjob-42");
    }

    #[tokio::test]
    async fn add_job_surfaces_server_error_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/jobs"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "project required"
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job = JobBuilder::new()
            .command("echo hi")
            .submitted_by("tester")
            .build();
        let err = client.add_job(job).await.unwrap_err();
        assert!(err.to_string().contains("project required"));
    }

    #[tokio::test]
    async fn add_jobs_returns_empty_vec_for_empty_input() {
        let server = MockServer::start().await;
        let client = client_for(&server);
        let resp = client.add_jobs(vec![]).await.expect("should succeed");
        assert!(resp.is_empty());
    }

    #[tokio::test]
    async fn add_jobs_returns_responses_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/jobs/batch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![
                serde_json::json!({"id": 1, "run_name": "gjob-1"}),
                serde_json::json!({"id": 2, "run_name": "gjob-2"}),
            ]))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let jobs = vec![
            JobBuilder::new()
                .command("echo a")
                .submitted_by("tester")
                .build(),
            JobBuilder::new()
                .command("echo b")
                .submitted_by("tester")
                .build(),
        ];
        let resp = client.add_jobs(jobs).await.expect("should add jobs");
        assert_eq!(resp.len(), 2);
        assert_eq!(resp[0].id, 1);
        assert_eq!(resp[1].id, 2);
    }

    // ── job action endpoints (finish/fail/cancel/hold/release) ─────────────

    #[tokio::test]
    async fn job_actions_succeed_on_2xx() {
        let server = MockServer::start().await;

        for (suffix, _action) in [
            ("finish", "finish job"),
            ("fail", "fail job"),
            ("cancel", "cancel job"),
            ("hold", "hold job"),
            ("release", "release job"),
        ] {
            Mock::given(method("POST"))
                .and(path(format!("/jobs/5/{suffix}")))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
        }

        let client = client_for(&server);
        client.finish_job(5).await.expect("finish should succeed");
        client.fail_job(5).await.expect("fail should succeed");
        client.cancel_job(5).await.expect("cancel should succeed");
        client.hold_job(5).await.expect("hold should succeed");
        client.release_job(5).await.expect("release should succeed");
    }

    #[tokio::test]
    async fn job_action_surfaces_error_on_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/jobs/5/cancel"))
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "error": "job is not running"
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client.cancel_job(5).await.unwrap_err();
        assert!(err.to_string().contains("job is not running"));
    }

    // ── update_job ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn update_job_returns_response_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/jobs/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "job": job_json(7, "Queued"),
                "updated_fields": ["priority", "gpus"]
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let request = UpdateJobRequest {
            priority: Some(5),
            gpus: Some(2),
            ..Default::default()
        };
        let resp = client
            .update_job(7, request)
            .await
            .expect("should update job");
        assert_eq!(resp.job.id, 7);
        assert_eq!(resp.updated_fields, vec!["priority", "gpus"]);
    }

    #[tokio::test]
    async fn update_job_surfaces_error_on_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/jobs/7"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid update"
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client
            .update_job(7, UpdateJobRequest::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid update"));
    }

    // ── get_job_log_path ───────────────────────────────────────────────────

    #[tokio::test]
    async fn get_job_log_path_returns_path_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/3/log"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!("/tmp/gflow-3.log")),
            )
            .mount(&server)
            .await;

        let client = client_for(&server);
        let path = client
            .get_job_log_path(3)
            .await
            .expect("request should succeed");
        assert_eq!(path, Some("/tmp/gflow-3.log".to_string()));
    }

    #[tokio::test]
    async fn get_job_log_path_returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/3/log"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let path = client
            .get_job_log_path(3)
            .await
            .expect("request should succeed");
        assert_eq!(path, None);
    }

    #[tokio::test]
    async fn get_job_log_path_errors_on_500() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/3/log"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client.get_job_log_path(3).await.unwrap_err();
        assert!(err.to_string().contains("500"));
    }

    // ── get_stats / get_info / get_health / get_health_with_pid ────────────

    #[tokio::test]
    async fn get_stats_returns_usage_stats() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/stats"))
            .and(query_param("user", "alice"))
            .and(query_param("since", "1000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user": "alice",
                "since": 1000,
                "total_jobs": 10,
                "completed_jobs": 8,
                "failed_jobs": 1,
                "cancelled_jobs": 1,
                "timeout_jobs": 0,
                "running_jobs": 0,
                "queued_jobs": 0,
                "avg_wait_secs": 5.0,
                "avg_runtime_secs": 100.0,
                "total_gpu_hours": 1.5,
                "jobs_with_gpus": 10,
                "avg_gpus_per_job": 1.0,
                "peak_gpu_usage": 1,
                "success_rate": 0.8,
                "top_jobs": []
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let stats = client
            .get_stats(Some("alice"), Some(1000))
            .await
            .expect("should get stats");
        assert_eq!(stats.total_jobs, 10);
        assert_eq!(stats.success_rate, 0.8);
    }

    #[tokio::test]
    async fn get_info_returns_scheduler_info() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "gpus": [
                    {"uuid": "gpu-0", "index": 0, "available": true},
                    {"uuid": "gpu-1", "index": 1, "available": false, "reason": "busy"}
                ],
                "allowed_gpu_indices": null,
                "gpu_allocation_strategy": "sequential"
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let info = client.get_info().await.expect("should get info");
        assert_eq!(info.gpus.len(), 2);
        assert!(info.gpus[0].available);
        assert!(!info.gpus[1].available);
        assert_eq!(
            info.gpu_allocation_strategy,
            GpuAllocationStrategy::Sequential
        );
    }

    #[tokio::test]
    async fn get_health_returns_status_code() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let status = client.get_health().await.expect("should get health");
        assert!(status.is_success());
    }

    #[tokio::test]
    async fn get_health_with_pid_returns_pid_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"pid": 12345})),
            )
            .mount(&server)
            .await;

        let client = client_for(&server);
        let pid = client
            .get_health_with_pid()
            .await
            .expect("should get health");
        assert_eq!(pid, Some(12345));
    }

    #[tokio::test]
    async fn get_health_with_pid_returns_none_on_error_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let pid = client
            .get_health_with_pid()
            .await
            .expect("request should succeed");
        assert_eq!(pid, None);
    }

    // ── resolve_dependency ─────────────────────────────────────────────────

    #[tokio::test]
    async fn resolve_dependency_returns_job_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/resolve-dependency"))
            .and(query_param("username", "alice"))
            .and(query_param("shorthand", "last"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"job_id": 17})),
            )
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job_id = client
            .resolve_dependency("alice", "last")
            .await
            .expect("should resolve dependency");
        assert_eq!(job_id, 17);
    }

    #[tokio::test]
    async fn resolve_dependency_surfaces_error_on_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/resolve-dependency"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": "no jobs found for user"
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client
            .resolve_dependency("alice", "last")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no jobs found for user"));
    }

    // ── GPU process management ─────────────────────────────────────────────

    #[tokio::test]
    async fn list_ignored_gpu_processes_returns_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gpu-processes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![
                serde_json::json!({"gpu_index": 0, "pid": 1234}),
                serde_json::json!({"gpu_index": 1, "pid": 5678}),
            ]))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let processes = client
            .list_ignored_gpu_processes()
            .await
            .expect("should list processes");
        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].pid, 1234);
    }

    #[tokio::test]
    async fn ignore_gpu_process_succeeds_on_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gpu-processes/ignore"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = client_for(&server);
        client
            .ignore_gpu_process(0, 1234)
            .await
            .expect("should ignore");
    }

    #[tokio::test]
    async fn unignore_gpu_process_succeeds_on_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gpu-processes/unignore"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = client_for(&server);
        client
            .unignore_gpu_process(0, 1234)
            .await
            .expect("should unignore");
    }

    // ── set_allowed_gpus ───────────────────────────────────────────────────

    #[tokio::test]
    async fn set_allowed_gpus_succeeds_on_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gpus"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = client_for(&server);
        client
            .set_allowed_gpus(Some(vec![0, 1]))
            .await
            .expect("should set gpus");
    }

    // ── set_group_max_concurrency ──────────────────────────────────────────

    #[tokio::test]
    async fn set_group_max_concurrency_returns_updated_count() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/groups/abc-123/max-concurrency"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"updated_jobs": 3})),
            )
            .mount(&server)
            .await;

        let client = client_for(&server);
        let count = client
            .set_group_max_concurrency("abc-123", 2)
            .await
            .expect("should set concurrency");
        assert_eq!(count, 3);
    }

    // ── reservations ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_reservation_returns_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/reservations"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"reservation_id": 9})),
            )
            .mount(&server)
            .await;

        let client = client_for(&server);
        let id = client
            .create_reservation("alice".into(), GpuSpec::Count(2), SystemTime::now(), 3600)
            .await
            .expect("should create reservation");
        assert_eq!(id, 9);
    }

    #[tokio::test]
    async fn list_reservations_returns_vec() {
        let server = MockServer::start().await;
        let reservation_json = serde_json::json!({
            "id": 1,
            "user": "alice",
            "gpu_spec": {"count": 2},
            "start_time": {"secs_since_epoch": 0, "nanos_since_epoch": 0},
            "duration": {"secs": 3600, "nanos": 0},
            "status": "Active",
            "created_at": {"secs_since_epoch": 0, "nanos_since_epoch": 0},
            "cancelled_at": null
        });
        Mock::given(method("GET"))
            .and(path("/reservations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![reservation_json]))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let reservations = client
            .list_reservations(None, None, false)
            .await
            .expect("should list reservations");
        assert_eq!(reservations.len(), 1);
        assert_eq!(reservations[0].id, 1);
        assert_eq!(reservations[0].user, CompactString::from("alice"));
    }

    #[tokio::test]
    async fn get_reservation_returns_some_when_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/reservations/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 5,
                "user": "bob",
                "gpu_spec": {"indices": [0, 1]},
                "start_time": {"secs_since_epoch": 0, "nanos_since_epoch": 0},
                "duration": {"secs": 1800, "nanos": 0},
                "status": "Pending",
                "created_at": {"secs_since_epoch": 0, "nanos_since_epoch": 0},
                "cancelled_at": null
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let reservation = client
            .get_reservation(5)
            .await
            .expect("request should succeed");
        assert!(reservation.is_some());
        let r = reservation.unwrap();
        assert_eq!(r.id, 5);
        assert_eq!(r.gpu_spec, GpuSpec::Indices(vec![0, 1]));
    }

    #[tokio::test]
    async fn get_reservation_returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/reservations/99"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let reservation = client
            .get_reservation(99)
            .await
            .expect("request should succeed");
        assert!(reservation.is_none());
    }

    #[tokio::test]
    async fn cancel_reservation_succeeds_on_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/reservations/3"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = client_for(&server);
        client
            .cancel_reservation(3)
            .await
            .expect("should cancel reservation");
    }

    #[tokio::test]
    async fn cancel_reservation_surfaces_error_on_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/reservations/3"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": "reservation not found"
            })))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client.cancel_reservation(3).await.unwrap_err();
        assert!(err.to_string().contains("reservation not found"));
    }

    // ── get_job_or_warn ────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_job_or_warn_returns_job_when_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(job_json(1, "Queued")))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job = get_job_or_warn(&client, 1)
            .await
            .expect("request should succeed");
        assert!(job.is_some());
    }

    #[tokio::test]
    async fn get_job_or_warn_returns_none_when_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jobs/99"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job = get_job_or_warn(&client, 99)
            .await
            .expect("request should succeed");
        assert!(job.is_none());
    }

    // ── error message extraction ───────────────────────────────────────────

    #[tokio::test]
    async fn error_message_falls_back_to_raw_body_when_not_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/jobs"))
            .respond_with(ResponseTemplate::new(500).set_body_string("disk full"))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let job = JobBuilder::new()
            .command("echo hi")
            .submitted_by("tester")
            .build();
        let err = client.add_job(job).await.unwrap_err();
        assert!(err.to_string().contains("disk full"));
    }
}
