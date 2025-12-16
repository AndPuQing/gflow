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
    use tabled::{settings::Style, Table, Tabled};

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

    // Group GPUs by availability state
    let available_gpus: Vec<_> = info.gpus.iter().filter(|g| g.available).collect();
    let allocated_gpus: Vec<_> = info.gpus.iter().filter(|g| !g.available).collect();

    // Define table structure
    #[derive(Tabled)]
    struct GpuRow {
        #[tabled(rename = "PARTITION")]
        partition: String,
        #[tabled(rename = "NODES")]
        nodes: String,
        #[tabled(rename = "GPUS")]
        gpus: String,
        #[tabled(rename = "STATE")]
        state: String,
        #[tabled(rename = "TIMELIMIT")]
        timelimit: String,
    }

    let mut rows = Vec::new();

    // Add available GPUs row
    if !available_gpus.is_empty() {
        let gpu_indices: Vec<String> = available_gpus.iter().map(|g| g.index.to_string()).collect();
        rows.push(GpuRow {
            partition: "gpu".to_string(),
            nodes: gpu_indices.join(","),
            gpus: format!("{}", available_gpus.len()),
            state: "idle".to_string(),
            timelimit: "infinite".to_string(),
        });
    }

    // Add allocated GPUs grouped by job
    let mut job_groups: HashMap<String, Vec<u32>> = HashMap::new();
    for g in &allocated_gpus {
        if let Some((job_id, run_name)) = usage.get(&g.index) {
            let job_key = format!("{}#{}", job_id, run_name);
            job_groups.entry(job_key).or_default().push(g.index);
        } else {
            job_groups
                .entry("allocated".to_string())
                .or_default()
                .push(g.index);
        }
    }

    // Add rows for each job group
    for (job_key, gpu_indices) in job_groups {
        let gpu_indices_str: Vec<String> = gpu_indices.iter().map(|g| g.to_string()).collect();
        rows.push(GpuRow {
            partition: "gpu".to_string(),
            nodes: gpu_indices_str.join(","),
            gpus: format!("{}", gpu_indices.len()),
            state: "allocated".to_string(),
            timelimit: job_key,
        });
    }

    // Print table
    if !rows.is_empty() {
        let table = Table::new(&rows).with(Style::empty()).to_string();
        println!("{}", table);
    }
}

#[cfg(test)]
mod tests {
    use gflow::core::job::JobBuilder;

    use super::*;

    // test print_gpu_allocation function
    #[test]
    fn test_print_gpu_allocation() {
        let info = gflow::core::info::SchedulerInfo {
            gpus: vec![
                gflow::core::info::GpuInfo {
                    index: 0,
                    available: true,
                    uuid: "GPU-0000".to_string(),
                },
                gflow::core::info::GpuInfo {
                    index: 1,
                    available: false,
                    uuid: "GPU-0001".to_string(),
                },
                gflow::core::info::GpuInfo {
                    index: 2,
                    available: false,
                    uuid: "GPU-0002".to_string(),
                },
            ],
            allowed_gpu_indices: None,
        };
        let jobs = vec![JobBuilder::new().build(), JobBuilder::new().build()];

        print_gpu_allocation(&info, &jobs);
    }
}
