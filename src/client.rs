use crate::core::info::SchedulerInfo;
use crate::core::job::Job;
use anyhow::Context;
use reqwest::Client as ReqwestClient;
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

    pub async fn add_job(&self, job: Job) -> anyhow::Result<JobSubmitResponse> {
        log::debug!("Adding job: {job:?}");
        let response = self
            .client
            .post(format!("{}/jobs", self.base_url))
            .json(&job)
            .send()
            .await
            .context("Failed to send job request")?;

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

    pub async fn get_job_log_path(&self, job_id: u32) -> anyhow::Result<String> {
        log::debug!("Getting log path for job {job_id}");
        let response = self
            .client
            .get(format!("{}/jobs/{}/log", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send get log path request")?;
        response
            .text()
            .await
            .context("Failed to read log path from response")
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
}
