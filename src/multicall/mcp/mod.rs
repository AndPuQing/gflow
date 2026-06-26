mod cli;
mod server;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::ffi::OsString;

pub async fn run(argv: Vec<OsString>) -> Result<()> {
    let args = cli::GMcpCli::parse_from(argv);

    if let Some(command) = args.command {
        match command {
            cli::Commands::Serve => {
                server::run(args.config, args.verbosity).await?;
            }
            cli::Commands::Completion { shell } => {
                crate::multicall::completion::handle_completion(
                    shell,
                    cli::GMcpCli::command(),
                    "mcp",
                )?;
            }
        }
        return Ok(());
    }

    let mut cmd = cli::GMcpCli::command();
    cmd.print_help()?;
    eprintln!();
    Ok(())
}
