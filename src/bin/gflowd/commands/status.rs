use anyhow::Result;
use gflow::{client::Client, tmux::is_session_exist};

pub async fn handle_status(config_path: &Option<std::path::PathBuf>) -> Result<()> {
    let session_exists = is_session_exist(super::TMUX_SESSION_NAME);

    if !session_exists {
        println!("Status: Not running");
        println!("The gflowd daemon is not running (tmux session not found).");
        return Ok(());
    }

    let config = gflow::config::load_config(config_path.as_ref()).unwrap_or_default();
    let client = Client::build(&config)?;

    match client.get_health().await {
        Ok(health) => {
            if health.is_success() {
                println!("Status: Running");
                println!(
                    "The gflowd daemon is running in tmux session '{}'.",
                    super::TMUX_SESSION_NAME
                );
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
