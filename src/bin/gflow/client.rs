use anyhow::{Context, Result};
use gflow_core::{get_config_temp_file, job::Job};
use reqwest::Response;
use std::fs;

const DEFAULT_PORT: u32 = 59000;

#[derive(Debug, Clone)]
pub struct Client {
    client: reqwest::Client,
    port: u32,
}

impl Client {
    pub fn build() -> Result<Self> {
        let port = Self::get_port()?;
        Ok(Self {
            client: reqwest::Client::new(),
            port,
        })
    }

    fn get_port() -> Result<u32> {
        let config_file = get_config_temp_file();

        if !config_file.exists() {
            log::warn!("Config file not found, using default port {}", DEFAULT_PORT);
            return Ok(DEFAULT_PORT);
        }

        fs::read_to_string(&config_file)
            .context("Failed to read config file")?
            .trim()
            .parse::<u32>()
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn list_jobs(&self) -> Result<Response> {
        let url = format!("http://localhost:{}/jobs", self.port);
        self.client
            .get(&url)
            .send()
            .await
            .context("Failed to send list jobs request")
    }

    pub async fn add_job(&self, job: Job) -> Result<Response> {
        log::debug!("Adding job: {:?}", job);

        let url = format!("http://localhost:{}/jobs", self.port);
        self.client
            .post(&url)
            .json(&job)
            .send()
            .await
            .context("Failed to send job request")
    }

    pub async fn update_job_state(
        &self,
        job_id: String,
        state: gflow_core::job::JobState,
    ) -> Result<Response> {
        log::debug!("Updating job {} to state {:?}", job_id, state);

        let url = format!("http://localhost:{}/jobs/{}", self.port, job_id);
        self.client
            .patch(&url)
            .json(&state)
            .send()
            .await
            .context("Failed to send update job state request")
    }
}
