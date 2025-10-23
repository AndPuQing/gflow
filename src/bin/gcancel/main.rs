mod cli;

use anyhow::Result;
use clap::Parser;
use gflow::{client::Client, config::load_config};

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GCancel::parse();
    let config = load_config(args.config.as_ref())?;
    let client = Client::build(&config)?;

    client.cancel_job(args.id).await?;
    println!("Job {} cancelled.", args.id);

    Ok(())
}
