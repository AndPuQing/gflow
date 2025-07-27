mod cli;

use anyhow::Result;
use clap::Parser;
use gflow::{client::Client, config::load_config};
use std::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GLogs::parse();
    let config = load_config(args.config.as_ref())?;
    let client = Client::build(&config)?;

    let response = client.get_job_log_path(args.id).await?;
    let log_path: String = response.text().await?;

    if args.follow {
        Command::new("tail")
            .arg("-f")
            .arg(log_path)
            .status()
            .expect("Failed to execute tail command");
    } else {
        Command::new("cat")
            .arg(log_path)
            .status()
            .expect("Failed to execute cat command");
    }

    Ok(())
}
