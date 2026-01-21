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
        #[tabled(rename = "GPUS")]
        gpus: String,
        #[tabled(rename = "NODES")]
        nodes: String,
        #[tabled(rename = "STATE")]
        state: String,
        #[tabled(rename = "JOB")]
        job: String,
    }

    let mut rows = Vec::new();

    // Add available GPUs row
    if !available_gpus.is_empty() {
        let gpu_indices: Vec<String> = available_gpus.iter().map(|g| g.index.to_string()).collect();
        rows.push(GpuRow {
            partition: "gpu".to_string(),
            gpus: format!("{}", available_gpus.len()),
            nodes: gpu_indices.join(","),
            state: "idle".to_string(),
            job: String::new(),
        });
    }

    // Add allocated GPUs grouped by job
    let mut job_groups: HashMap<(u32, String), Vec<u32>> = HashMap::new();
    for g in &allocated_gpus {
        if let Some((job_id, run_name)) = usage.get(&g.index) {
            job_groups
                .entry((*job_id, run_name.clone()))
                .or_default()
                .push(g.index);
        } else {
            job_groups
                .entry((0, "unknown".to_string()))
                .or_default()
                .push(g.index);
        }
    }

    // Sort job groups by the minimum GPU index for consistent output
    let mut sorted_jobs: Vec<_> = job_groups.into_iter().collect();
    sorted_jobs.sort_by_key(|(_, gpu_indices)| *gpu_indices.iter().min().unwrap_or(&u32::MAX));

    // Add rows for each job group
    for ((job_id, run_name), mut gpu_indices) in sorted_jobs {
        gpu_indices.sort_unstable();
        let gpu_indices_str: Vec<String> = gpu_indices.iter().map(|g| g.to_string()).collect();
        let job_display = if job_id == 0 {
            String::new()
        } else {
            format!("{} ({})", job_id, run_name)
        };
        rows.push(GpuRow {
            partition: "gpu".to_string(),
            gpus: format!("{}", gpu_indices.len()),
            nodes: gpu_indices_str.join(","),
            state: "allocated".to_string(),
            job: job_display,
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
