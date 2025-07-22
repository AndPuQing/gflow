use crate::{cli, client::Client};
use anyhow::{Context, Result};

pub(crate) async fn handle_fail(fail_args: cli::FailArgs) -> Result<()> {
    let client = Client::build().context("Failed to build client")?;

    client
        .update_job_state(fail_args.name, gflow_core::job::JobState::Failed)
        .await
        .context("Failed to finish job")?;
    Ok(())
}
