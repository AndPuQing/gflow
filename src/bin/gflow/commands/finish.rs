use crate::{cli, client::Client};
use anyhow::{Context, Result};

pub(crate) async fn handle_finish(finish_args: cli::FinishArgs) -> Result<()> {
    let client = Client::build().context("Failed to build client")?;

    client
        .finish_job(finish_args.name)
        .await
        .context("Failed to finish job")?;
    Ok(())
}
