use anyhow::Context;
use gflow::core::job::Job;
use reqwest::{Client as ReqwestClient, Response};

#[derive(Debug, Clone)]
pub struct Client {
    client: ReqwestClient,
    base_url: String,
}

impl Client {
    pub fn build(config: &config::Config) -> anyhow::Result<Self> {
        let host = config
            .get_string("host")
            .unwrap_or_else(|_| "localhost".to_string());
        let port = config.get_int("port").unwrap_or(59000);
        let base_url = format!("http://{host}:{port}");
        let client = ReqwestClient::new();
        Ok(Self { client, base_url })
    }

    pub async fn list_jobs(&self) -> anyhow::Result<Response> {
        self.client
            .get(format!("{}/jobs", self.base_url))
            .send()
            .await
            .context("Failed to send list jobs request")
    }

    pub async fn add_job(&self, job: Job) -> anyhow::Result<Response> {
        log::debug!("Adding job: {:?}", job);
        self.client
            .post(format!("{}/jobs", self.base_url))
            .json(&job)
            .send()
            .await
            .context("Failed to send job request")
    }

    pub async fn finish_job(&self, job_id: u32) -> anyhow::Result<Response> {
        log::debug!("Finishing job {}", job_id);
        self.client
            .post(format!("{}/jobs/{}/finish", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send finish job request")
    }

    pub async fn fail_job(&self, job_id: u32) -> anyhow::Result<Response> {
        log::debug!("Failing job {}", job_id);
        self.client
            .post(format!("{}/jobs/{}/fail", self.base_url, job_id))
            .send()
            .await
            .context("Failed to send fail job request")
    }
}
