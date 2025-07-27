mod cli;

use anyhow::Result;
use clap::Parser;
use gflow::tmux::TmuxSession;
use tmux_interface::{KillSession, Tmux};

pub static TMUX_SESSION_NAME: &str = "gflow_server";

fn main() -> Result<()> {
    let args = cli::GCtl::parse();

    match args.command {
        cli::Commands::Start => {
            let session = TmuxSession::new(TMUX_SESSION_NAME.to_string());
            session.send_command("gflowd -vvv");
            println!("gflowd started.");
        }
        cli::Commands::Stop => {
            Tmux::with_command(KillSession::new().target_session(TMUX_SESSION_NAME))
                .output()
                .unwrap();
            println!("gflowd stopped.");
        }
        cli::Commands::Status => {
            println!("Checking gflowd status...");
            // In the future, this will communicate with the daemon
            // to check its actual status.
            println!("gflowd status check is not yet implemented.");
        }
    }

    Ok(())
}
