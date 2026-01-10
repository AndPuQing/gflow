use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use gflow::client::Client;

pub mod set_gpus;
pub mod set_group_max_concurrency;
pub mod show_gpus;

use crate::cli;

pub async fn handle_commands(client: &Client, command: cli::Commands) -> Result<()> {
    match command {
        cli::Commands::SetGpus { gpu_spec } => {
            set_gpus::handle_set_gpus(client, &gpu_spec).await?;
        }
        cli::Commands::ShowGpus => {
            show_gpus::handle_show_gpus(client).await?;
        }
        cli::Commands::SetLimit {
            job_or_group_id,
            limit,
        } => {
            set_group_max_concurrency::handle_set_group_max_concurrency(
                client,
                &job_or_group_id,
                limit,
            )
            .await?;
        }
        cli::Commands::Completion { shell } => {
            let mut cmd = cli::GCtl::command();
            generate(shell, &mut cmd, "gctl", &mut std::io::stdout());
        }
    }

    Ok(())
}
