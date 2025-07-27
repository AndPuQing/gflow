mod cli;
use anyhow::Result;
use clap::Parser;
use gflow::{client::Client, config::load_config, core::job::Job};

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GQueue::parse();
    let config = load_config(args.config.as_ref())?;

    let client = Client::build(&config)?;
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

    Ok(())
}
