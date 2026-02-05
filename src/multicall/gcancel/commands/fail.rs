use anyhow::Result;
use gflow::client::Client;

pub async fn handle_fail(client: &Client, job_id: u32) -> Result<()> {
    client.fail_job(job_id).await?;

    Ok(())
}
