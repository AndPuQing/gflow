use anyhow::Result;
use gflow::core::job::Job;

use crate::{cli::ListArgs, client::Client};

pub(crate) async fn handle_list(config: &config::Config, list_args: ListArgs) -> Result<()> {
    if list_args.tui {
        let _ = crate::tui::show_tui();
    } else {
        let client = Client::build(config)?;
        let jobs = client
            .list_jobs()
            .await?
            .json::<Vec<Job>>()
            .await
            .unwrap_or_default();
        if jobs.is_empty() {
            println!("No jobs found.");
            return Ok(());
        }
        println!(
            "{:<5} {:<20} {:<30} {:<10} {:<10}",
            "ID", "RunName", "Command", "AllocGPUS", "Status"
        );
        for job in jobs {
            println!(
                "{:<5} {:<20} {:<30} {:<10} {:<10}",
                job.id,
                job.run_name.as_deref().unwrap_or("-"),
                job.command.as_deref().unwrap_or("-"),
                job.gpus,
                job.state.to_string()
            );
        }
    }
    Ok(())
}
