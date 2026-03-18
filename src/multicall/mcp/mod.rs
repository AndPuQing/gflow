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
                let mut cmd = cli::GMcpCli::command();
                let _ = crate::multicall::completion::generate_to_stdout(shell, &mut cmd, "mcp");
            }
        }
        return Ok(());
    }

    let mut cmd = cli::GMcpCli::command();
    cmd.print_help()?;
    eprintln!();
    Ok(())
}
