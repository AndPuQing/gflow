use crate::client::Client;
use anyhow::{Context, Result};
use gflow::job::Job;

pub(crate) async fn handle_list() -> Result<()> {
    let client = Client::build().context("Failed to build client")?;
    let response = client.list_jobs().await.context("Failed to list jobs")?;

    let jobs = response
        .json::<Vec<Job>>()
        .await
        .context("Failed to parse response")?;
    let _ = crate::tui::show_tui(&jobs);
    Ok(())
}
