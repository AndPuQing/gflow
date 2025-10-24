use crate::cli::Commands;
use anyhow::{Context, Result};
use gflow::client::Client;
use gflow::config::Config;
use gflow::core::info::SchedulerInfo;
use gflow::core::job::{Job, JobState};

pub async fn handle_command(config: &Config, command: Commands) -> Result<()> {
    let client = Client::build(config)?;
    match command {
        Commands::Partition(_args) => {
            match fetch_info_and_jobs(&client).await {
                Ok((info, _jobs)) => print_partition_summary(&info),
                Err(_e) => {
                    // Daemon unavailable: show a sinfo-like DOWN line
                    print_partition_down();
                }
            }
        }
        Commands::Gpu(_args) => match fetch_info_and_jobs(&client).await {
            Ok((info, jobs)) => print_gpu_allocation(&info, &jobs),
            Err(e) => {
                eprintln!("ginfo: daemon not reachable: {e}");
            }
        },
    }
    Ok(())
}

async fn fetch_info_and_jobs(client: &Client) -> Result<(SchedulerInfo, Vec<Job>)> {
    let info = client
        .get_info()
        .await
        .context("Failed to get scheduler info")?;
    let jobs = client.list_jobs().await.context("Failed to list jobs")?;
    Ok((info, jobs))
}

fn print_partition_summary(info: &SchedulerInfo) {
    // sinfo-like summary for a single-node, single "gpu" partition
    let total_gpus = info.gpus.len() as u32;
    let free_gpus = info.gpus.iter().filter(|g| g.available).count() as u32;
    let used_gpus = total_gpus.saturating_sub(free_gpus);
    let nodes = 1u32; // single node scheduler currently

    // Determine a sinfo-like state
    let state = if total_gpus == 0 {
        "down"
    } else if used_gpus == 0 {
        "idle"
    } else if free_gpus == 0 {
        "alloc"
    } else {
        "mix"
    };

    // Header similar to sinfo
    println!(
        "{:<10} {:<6} {:<5} {:<6} {:<10} {:<10} {:<10}",
        "PARTITION", "AVAIL", "NODES", "STATE", "GRES", "GRES_USED", "GRES_FREE"
    );
    println!(
        "{:<10} {:<6} {:<5} {:<6} {:<10} {:<10} {:<10}",
        "gpu",
        "up",
        nodes,
        state,
        format!("gpu:{}", total_gpus),
        format!("gpu:{}", used_gpus),
        format!("gpu:{}", free_gpus)
    );
}

fn print_gpu_allocation(info: &SchedulerInfo, jobs: &[Job]) {
    // Build a reverse index: gpu_index -> Option<(job_id, run_name)>
    use std::collections::HashMap;
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

fn print_partition_down() {
    println!(
        "{:<10} {:<6} {:<5} {:<6} {:<10} {:<10} {:<10}",
        "PARTITION", "AVAIL", "NODES", "STATE", "GRES", "GRES_USED", "GRES_FREE"
    );
    println!(
        "{:<10} {:<6} {:<5} {:<6} {:<10} {:<10} {:<10}",
        "gpu", "down", 0, "down", "gpu:0", "gpu:0", "gpu:0"
    );
}
