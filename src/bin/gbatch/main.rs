use anyhow::Result;
use clap::Parser;
use commands::handle_commands;
use gflow::config::load_config;
use std::io::{self, IsTerminal};
mod cli;
mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GBatch::parse();
    let config = load_config(args.config.as_ref())?;

    if let Some(commands) = args.commands {
        handle_commands(&config, commands).await
    } else {
        // Check if stdin is available (not a terminal)
        let stdin_available = !io::stdin().is_terminal();

        // Check if user explicitly requested stdin with "-"
        let explicit_stdin =
            args.add_args.script_or_command.len() == 1 && args.add_args.script_or_command[0] == "-";

        // Validate that either we have args, stdin is available, "--param" is specified, or "-" was specified
        if args.add_args.script_or_command.is_empty()
            && !stdin_available
            && args.add_args.param.is_empty()
        {
            anyhow::bail!("The following required arguments were not provided:\n  <SCRIPT_OR_COMMAND>...\n\nUsage: gbatch <SCRIPT_OR_COMMAND>...\n       gbatch < script.sh\n       gbatch -\n\nFor more information, try 'gbatch --help'");
        }

        // Use stdin only if:
        // 1. Explicitly requested with "-", OR
        // 2. stdin is available AND no command args provided (or only "-")
        let use_stdin =
            explicit_stdin || (stdin_available && args.add_args.script_or_command.is_empty());

        commands::add::handle_add(&config, args.add_args, use_stdin).await
    }
}
