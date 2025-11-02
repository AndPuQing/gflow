use crate::core::info::SchedulerInfo;
use crate::core::job::Job;
use anyhow::{anyhow, Context};
use reqwest::{Client as ReqwestClient, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSubmitResponse {
    pub id: u32,
    pub run_name: String,
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

    pub async fn get_job(&self, job_id: u32) -> anyhow::Result<Option<Job>> {
        log::debug!("Getting job {job_id}");
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
        log::debug!("Adding job: {job:?}");
        let response = self
            .client
            .post(format!("{}/jobs", self.base_url))
            .json(&job)
            .send()
            .await
            .context("Failed to send job request")?;

        // Check if the response is successful
        if !response.status().is_success() {
            // Try to extract error message from response
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("Unknown error"));

            // Try to parse as JSON with error field
            if let Ok(json_error) = serde_json::from_str::<serde_json::Value>(&error_body) {
                if let Some(error_msg) = json_error.get("error").and_then(|e| e.as_str()) {
                    return Err(anyhow::anyhow!("{}", error_msg));
                }
            }

            return Err(anyhow::anyhow!("Failed to add job: {}", error_body));
        }

        let job_response: JobSubmitResponse = response
            .json()
            .await
            .context("Failed to parse response json")?;
        Ok(job_response)
    }

    pub async fn finish_job(&self, job_id: u32) -> anyhow::Result<()> {
        log::debug!("Finishing job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/finish", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send finish job request")?;
        Ok(())
    }

    pub async fn fail_job(&self, job_id: u32) -> anyhow::Result<()> {
        log::debug!("Failing job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/fail", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send fail job request")?;
        Ok(())
    }

    pub async fn cancel_job(&self, job_id: u32) -> anyhow::Result<()> {
        log::debug!("Cancelling job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/cancel", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send cancel job request")?;
        Ok(())
    }

    pub async fn hold_job(&self, job_id: u32) -> anyhow::Result<()> {
        log::debug!("Holding job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/hold", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send hold job request")?;
        Ok(())
    }

    pub async fn release_job(&self, job_id: u32) -> anyhow::Result<()> {
        log::debug!("Releasing job {job_id}");
        self.client
            .post(format!("{}/jobs/{}/release", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send release job request")?;
        Ok(())
    }

    pub async fn get_job_log_path(&self, job_id: u32) -> anyhow::Result<Option<String>> {
        log::debug!("Getting log path for job {job_id}");
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
        log::debug!("Getting scheduler info");
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
        log::debug!("Getting health status");
        let health = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .context("Failed to send health request")?
            .status();
        Ok(health)
    }

    pub async fn resolve_dependency(&self, username: &str, shorthand: &str) -> anyhow::Result<u32> {
        log::debug!(
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
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("Unknown error"));

            if let Ok(json_error) = serde_json::from_str::<serde_json::Value>(&error_body) {
                if let Some(error_msg) = json_error.get("error").and_then(|e| e.as_str()) {
                    return Err(anyhow::anyhow!("{}", error_msg));
                }
            }

            return Err(anyhow::anyhow!(
                "Failed to resolve dependency: {}",
                error_body
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
}
