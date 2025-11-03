use anyhow::Result;
use clap::Parser;
use commands::handle_commands;
use gflow::config::load_config;
mod cli;
mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GBatch::parse();
    let config = load_config(args.config.as_ref())?;

    if let Some(commands) = args.commands {
        handle_commands(&config, commands).await
    } else {
        // Validate that script_or_command is provided when not using a subcommand
        if args.add_args.script_or_command.is_empty() {
            anyhow::bail!("The following required arguments were not provided:\n  <SCRIPT_OR_COMMAND>...\n\nUsage: gbatch <SCRIPT_OR_COMMAND>...\n\nFor more information, try 'gbatch --help'");
        }
        commands::add::handle_add(&config, args.add_args).await
    }
}
