mod cli;

use anyhow::{Context, Result};
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
        cli::Commands::Info => {
            show_info(&args.config).await?;
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

async fn show_info(config_path: &Option<std::path::PathBuf>) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref()).unwrap_or_default();
    let client = gflow::client::Client::build(&config)?;

    match fetch_info_and_jobs(&client).await {
        Ok((info, jobs)) => {
            print_gpu_allocation(&info, &jobs);
        }
        Err(e) => {
            eprintln!("gctl: daemon not reachable: {e}");
        }
    }
    Ok(())
}

async fn fetch_info_and_jobs(
    client: &gflow::client::Client,
) -> Result<(gflow::core::info::SchedulerInfo, Vec<gflow::core::job::Job>)> {
    let info = client
        .get_info()
        .await
        .context("Failed to get scheduler info")?;
    let jobs = client.list_jobs().await.context("Failed to list jobs")?;
    Ok((info, jobs))
}

fn print_gpu_allocation(info: &gflow::core::info::SchedulerInfo, jobs: &[gflow::core::job::Job]) {
    use gflow::core::job::JobState;
    use std::collections::HashMap;

    // Build a reverse index: gpu_index -> Option<(job_id, run_name)>
    let mut usage: HashMap<u32, (u32, String)> = HashMap::new();
    for j in jobs.iter().filter(|j| j.state == JobState::Running) {
        if let Some(gpu_ids) = &j.gpu_ids {
            for &idx in gpu_ids {
                let name = j
                    .run_name
                    .clone()
                    .unwrap_or_else(|| "<unknown>".to_string());
                usage.insert(idx, (j.id, name));
            }
        }
    }

    println!("GPU allocation");
    println!("--------------");
    println!("{:<6} {:<12} {:<10} USED BY", "INDEX", "UUID", "AVAILABLE");
    for g in &info.gpus {
        let short_uuid = if g.uuid.len() > 12 {
            &g.uuid[..12]
        } else {
            &g.uuid
        };
        if let Some((job_id, run_name)) = usage.get(&g.index) {
            println!(
                "{:<6} {:<12} {:<10} job #{}, {}",
                g.index,
                short_uuid,
                if g.available { "yes" } else { "no" },
                job_id,
                run_name
            );
        } else {
            println!(
                "{:<6} {:<12} {:<10} -",
                g.index,
                short_uuid,
                if g.available { "yes" } else { "no" }
            );
        }
    }
}
