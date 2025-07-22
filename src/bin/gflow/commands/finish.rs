use crate::{cli, client::Client};
use anyhow::{Context, Result};

pub(crate) async fn handle_finish(finish_args: cli::FinishArgs) -> Result<()> {
    let client = Client::build().context("Failed to build client")?;

    client
        .update_job_state(finish_args.name, gflow_core::job::JobState::Finished)
        .await
        .context("Failed to finish job")?;
    Ok(())
}
