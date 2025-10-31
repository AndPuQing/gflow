use anyhow::{Context, Result};
use gflow::client::Client;

pub async fn handle_info(config_path: &Option<std::path::PathBuf>) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref()).unwrap_or_default();
    let client = Client::build(&config)?;

    match fetch_info_and_jobs(&client).await {
        Ok((info, jobs)) => {
            print_gpu_allocation(&info, &jobs);
        }
        Err(e) => {
            eprintln!("ginfo: daemon not reachable: {e}");
        }
    }
    Ok(())
}

async fn fetch_info_and_jobs(
    client: &Client,
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
