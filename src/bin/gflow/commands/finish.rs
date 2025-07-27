use crate::{cli, client::Client};
use anyhow::{Context, Result};

pub(crate) async fn handle_finish(
    config: &config::Config,
    finish_args: cli::FinishArgs,
) -> Result<()> {
    let client = Client::build(config).context("Failed to build client")?;

    client
        .finish_job(finish_args.id)
        .await
        .context("Failed to finish job")?;
    Ok(())
}
