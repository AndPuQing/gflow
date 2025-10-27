use crate::cli::CancelCommand;

pub mod cancel;
pub mod fail;
pub mod finish;

pub async fn handle_commands(
    config: &gflow::config::Config,
    command: CancelCommand,
) -> anyhow::Result<()> {
    let client = gflow::client::Client::build(config)?;

    match command {
        CancelCommand::Cancel { ids, dry_run } => {
            cancel::handle_cancel(&client, &ids, dry_run).await?;
        }
        CancelCommand::Finish { id } => {
            finish::handle_finish(&client, id).await?;
        }
        CancelCommand::Fail { id } => {
            fail::handle_fail(&client, id).await?;
        }
    }

    Ok(())
}
