mod cli;

use anyhow::Result;
use clap::Parser;
use gflow::{client::Client, config::load_config};

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GSignal::parse();
    let config = load_config(args.config.as_ref())?;
    let client = Client::build(&config)?;

    match args.command {
        cli::Commands::Finish(finish_args) => {
            client.finish_job(finish_args.id).await?;
            println!("Job {} finished.", finish_args.id);
        }
        cli::Commands::Fail(fail_args) => {
            client.fail_job(fail_args.id).await?;
            println!("Job {} failed.", fail_args.id);
        }
    }

    Ok(())
}
