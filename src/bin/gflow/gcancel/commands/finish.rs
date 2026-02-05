use anyhow::Result;
use gflow::client::Client;

pub async fn handle_finish(client: &Client, job_id: u32) -> Result<()> {
    client.finish_job(job_id).await?;

    Ok(())
}
