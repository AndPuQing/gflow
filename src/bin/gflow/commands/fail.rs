use crate::{cli, client::Client};
use anyhow::{Context, Result};

pub(crate) async fn handle_fail(config: &config::Config, fail_args: cli::FailArgs) -> Result<()> {
    let client = Client::build(config).context("Failed to build client")?;

    client
        .fail_job(fail_args.id)
        .await
        .context("Failed to finish job")?;
    Ok(())
}
