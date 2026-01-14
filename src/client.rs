use crate::core::info::SchedulerInfo;
use crate::core::job::{DependencyMode, Job};
use anyhow::{anyhow, Context};
use reqwest::{Client as ReqwestClient, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

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
    pub depends_on_ids: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_mode: Option<Option<DependencyMode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_cancel_on_dependency_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<Option<usize>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobResponse {
    pub job: Job,
    pub updated_fields: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Client {
    client: ReqwestClient,
    base_url: String,
}

impl Client {
    pub fn build(config: &crate::config::Config) -> anyhow::Result<Self> {
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
            .context("Failed to send list jobs request")?
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
    ///
    /// Returns all matching jobs from the database, not just in-memory jobs.
    pub async fn list_jobs_with_query(
        &self,
        states: Option<String>,
        user: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
        created_after: Option<i64>,
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

        if !params.is_empty() {
            request = request.query(&params);
        }

        let response = request
            .send()
            .await
            .context("Failed to send list jobs request")?;

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
            .context("Failed to send get job request")?;

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
            .context("Failed to send job request")?;

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
            .context("Failed to send batch jobs request")?;

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
        self.client
            .post(format!("{}/jobs/{}/finish", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send finish job request")?;
        Ok(())
    }

    pub async fn fail_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Failing job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/fail", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send fail job request")?;
        Ok(())
    }

    pub async fn cancel_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Cancelling job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/cancel", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send cancel job request")?;
        Ok(())
    }

    pub async fn hold_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Holding job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/hold", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send hold job request")?;
        Ok(())
    }

    pub async fn release_job(&self, job_id: u32) -> anyhow::Result<()> {
        tracing::debug!("Releasing job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/release", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send release job request")?;
        Ok(())
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
            .context("Failed to send update job request")?;

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
            .context("Failed to send get log path request")?;
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

    pub async fn get_info(&self) -> anyhow::Result<SchedulerInfo> {
        tracing::debug!("Getting scheduler info");
        let info = self
            .client
            .get(format!("{}/info", self.base_url))
            .send()
            .await
            .context("Failed to send info request")?
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
            .context("Failed to send health request")?
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
            .context("Failed to send health request")?;

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
            .context("Failed to send resolve dependency request")?;

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
            .context("Failed to send set GPUs request")?;

        if !response.status().is_success() {
            let error_msg = Self::extract_error_message(response).await;
            return Err(anyhow!("Failed to set GPU configuration: {}", error_msg));
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
            .context("Failed to send set group max_concurrency request")?;

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
}
