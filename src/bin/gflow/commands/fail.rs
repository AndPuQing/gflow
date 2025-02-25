use crate::{cli, client::Client};
use anyhow::{Context, Result};

pub(crate) async fn handle_fail(fail_args: cli::FailArgs) -> Result<()> {
    let client = Client::build().context("Failed to build client")?;

    client
        .fail_job(fail_args.name)
        .await
        .context("Failed to finish job")?;
    Ok(())
}
