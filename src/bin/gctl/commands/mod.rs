use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use gflow::client::Client;
use gflow::config::Config;

pub mod reserve_cancel;
pub mod reserve_create;
pub mod reserve_get;
pub mod reserve_list;
pub mod set_gpus;
pub mod set_group_max_concurrency;
pub mod show_gpus;

use crate::cli;

pub async fn handle_commands(
    client: &Client,
    config: &Config,
    command: cli::Commands,
) -> Result<()> {
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
        cli::Commands::Reserve { command } => match command {
            cli::ReserveCommands::Create {
                user,
                gpus,
                gpu_spec,
                start,
                duration,
                timezone,
            } => {
                reserve_create::handle_reserve_create(
                    client,
                    config,
                    reserve_create::ReserveCreateParams {
                        user: &user,
                        gpus: gpus.as_ref().copied(),
                        gpu_spec: gpu_spec.as_deref(),
                        start: &start,
                        duration: &duration,
                        timezone: timezone.as_deref(),
                    },
                )
                .await?;
            }
            cli::ReserveCommands::List {
                user,
                status,
                active,
                timeline,
                range,
                from,
                to,
            } => {
                reserve_list::handle_reserve_list(
                    client,
                    config,
                    user,
                    status,
                    active,
                    timeline,
                    reserve_list::TimelineRangeOpts { range, from, to },
                )
                .await?;
            }
            cli::ReserveCommands::Get { id } => {
                reserve_get::handle_reserve_get(client, config, id).await?;
            }
            cli::ReserveCommands::Cancel { id } => {
                reserve_cancel::handle_reserve_cancel(client, id).await?;
            }
        },
        cli::Commands::Completion { shell } => {
            let mut cmd = cli::GCtl::command();
            generate(shell, &mut cmd, "gctl", &mut std::io::stdout());
        }
    }

    Ok(())
}
