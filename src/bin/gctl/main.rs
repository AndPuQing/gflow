mod cli;

use anyhow::Result;
use clap::Parser;
use gflow::tmux::{is_session_exist, TmuxSession};
use tmux_interface::{KillSession, Tmux};

pub static TMUX_SESSION_NAME: &str = "gflow_server";

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GCtl::parse();

    match args.command {
        cli::Commands::Up => {
            let session = TmuxSession::new(TMUX_SESSION_NAME.to_string());
            session.send_command("gflowd -vvv");
            println!("gflowd started.");
        }
        cli::Commands::Down => {
            if let Err(e) =
                Tmux::with_command(KillSession::new().target_session(TMUX_SESSION_NAME)).output()
            {
                eprintln!("Failed to stop gflowd: {e}");
            } else {
                println!("gflowd stopped.");
            }
        }
        cli::Commands::Status => {
            check_status(&args.config).await?;
        }
    }

    Ok(())
}

async fn check_status(config_path: &Option<std::path::PathBuf>) -> Result<()> {
    let session_exists = is_session_exist(TMUX_SESSION_NAME);

    if !session_exists {
        println!("Status: Not running");
        println!("The gflowd daemon is not running (tmux session not found).");
        return Ok(());
    }

    // Try to get daemon info
    let config = gflow::config::load_config(config_path.as_ref()).unwrap_or_default();
    let client = gflow::client::Client::build(&config)?;

    match client.get_health().await {
        Ok(health) => {
            if health.is_success() {
                println!("Status: Running");
                println!("The gflowd daemon is running in tmux session '{TMUX_SESSION_NAME}'.");
            } else {
                println!("Status: Unhealthy");
                eprintln!("The gflowd daemon responded to the health check but is not healthy.");
            }
        }
        Err(e) => {
            println!("Status: Not Running");
            eprintln!("Failed to connect to gflowd daemon: {e}");
        }
    }
    Ok(())
}
