use anyhow::Result;
use gflow::client::Client;

pub async fn handle_info(config_path: &Option<std::path::PathBuf>) -> Result<()> {
    let client = gflow::create_client_or_default(config_path)?;

    let (info, jobs) = fetch_info_and_jobs(&client).await?;
    print_gpu_allocation(&info, &jobs);
    Ok(())
}

async fn fetch_info_and_jobs(
    client: &Client,
) -> Result<(gflow::core::info::SchedulerInfo, Vec<gflow::core::job::Job>)> {
    let info = client.get_info().await?;
    let jobs = client.list_jobs().await?;
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
                    .as_ref()
                    .map(|s| s.to_string())
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
        #[tabled(rename = "JOB(REASON)")]
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
            // GPU is allocated but not by a gflow job - use reason if available
            let reason = g.reason.clone().unwrap_or_else(|| "unknown".to_string());
            job_groups.entry((0, reason)).or_default().push(g.index);
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
            // Non-gflow job - show reason in parentheses
            format!("({})", run_name)
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

    print_unmanaged_processes(info);
    print_ignore_overrides(info);
}

/// List the non-gflow processes holding GPUs back, with the exact command that
/// releases each one.
///
/// This is the discovery surface the operators actually look at: the reason
/// string in the table above says a GPU is blocked, but only here do they see
/// how much memory the process holds, whether it is doing any work, and how to
/// get the card back.
fn print_unmanaged_processes(info: &gflow::core::info::SchedulerInfo) {
    use gflow::core::info::UnmanagedGpuProcess;
    use tabled::{settings::Style, Table, Tabled};

    #[derive(Tabled)]
    struct ProcessRow {
        #[tabled(rename = "GPU")]
        gpu: u32,
        #[tabled(rename = "PID")]
        pid: u32,
        #[tabled(rename = "GPU MEM")]
        memory: String,
        #[tabled(rename = "UTIL")]
        utilization: String,
        #[tabled(rename = "AGE")]
        age: String,
        #[tabled(rename = "NOTE")]
        note: String,
        #[tabled(rename = "RELEASE")]
        release: String,
    }

    let format_process = |gpu_index: u32, process: &UnmanagedGpuProcess| ProcessRow {
        gpu: gpu_index,
        pid: process.pid,
        memory: process
            .used_memory_mb
            .map(gflow::utils::format_memory)
            .unwrap_or_else(|| "?".to_string()),
        utilization: process
            .utilization_percent
            .map(|util| format!("{}%", util))
            .unwrap_or_else(|| "?".to_string()),
        age: process
            .age_secs
            .map(|secs| gflow::utils::format_duration_compact(std::time::Duration::from_secs(secs)))
            .unwrap_or_else(|| "?".to_string()),
        note: if process.is_idle_leftover() {
            "idle leftover".to_string()
        } else {
            String::new()
        },
        release: process.release_command(gpu_index),
    };

    let mut rows: Vec<ProcessRow> = info
        .gpus
        .iter()
        .flat_map(|gpu| {
            gpu.unmanaged_processes
                .iter()
                .map(move |process| format_process(gpu.index, process))
        })
        .collect();

    // Show idle leftovers first: those are the ones blocking a card for no
    // good reason, so they should be the first thing the operator sees.
    rows.sort_by(|left, right| {
        let left_idle = left.note == "idle leftover";
        let right_idle = right.note == "idle leftover";
        right_idle
            .cmp(&left_idle)
            .then_with(|| left.gpu.cmp(&right.gpu))
            .then_with(|| left.pid.cmp(&right.pid))
    });

    if rows.is_empty() {
        return;
    }

    println!();
    println!("Non-gflow GPU processes (gflow will not allocate a GPU while these are attached):");
    let table = Table::new(&rows).with(Style::blank()).to_string();
    println!("{}", table);
    println!("Release a card without touching the process: run the RELEASE command for that PID.");
}

/// Show the runtime-only ignore overrides currently in effect, so an operator
/// can see what will disappear when the daemon restarts.
fn print_ignore_overrides(info: &gflow::core::info::SchedulerInfo) {
    if info.ignored_gpu_processes.is_empty() {
        return;
    }

    println!();
    println!("Active GPU process ignore overrides (runtime-only; cleared when gflowd restarts):");
    for process in &info.ignored_gpu_processes {
        println!(
            "  gpu={} pid={}  (undo: gctl gpu-process unignore --gpu {} --pid {})",
            process.gpu_index, process.pid, process.gpu_index, process.pid
        );
    }
}

#[cfg(test)]
mod tests {
    use gflow::core::gpu_allocation::GpuAllocationStrategy;
    use gflow::core::info::{GpuInfo, IgnoredGpuProcess, SchedulerInfo, UnmanagedGpuProcess};
    use gflow::core::job::JobBuilder;

    use super::*;

    fn gpu(index: u32, available: bool, reason: Option<&str>) -> GpuInfo {
        GpuInfo {
            index,
            available,
            uuid: format!("GPU-{:04}", index),
            reason: reason.map(ToString::to_string),
            unmanaged_processes: Vec::new(),
        }
    }

    fn scheduler_info(gpus: Vec<GpuInfo>) -> SchedulerInfo {
        SchedulerInfo {
            executor: String::new(),
            gpus,
            allowed_gpu_indices: None,
            gpu_allocation_strategy: GpuAllocationStrategy::Sequential,
            ignored_gpu_processes: Vec::new(),
        }
    }

    // test print_gpu_allocation function
    #[test]
    fn test_print_gpu_allocation() {
        let info = scheduler_info(vec![
            gpu(0, true, None),
            gpu(1, false, None),
            gpu(2, false, Some("Unmanaged")),
        ]);
        let jobs = vec![JobBuilder::new().build(), JobBuilder::new().build()];

        print_gpu_allocation(&info, &jobs);
    }

    #[test]
    fn gpu_info_deserializes_without_the_new_fields() {
        // Older daemons do not send `unmanaged_processes`; clients must not break.
        let gpu: GpuInfo = serde_json::from_str(
            r#"{"uuid":"GPU-0","index":0,"available":false,"reason":"unmanaged(pid=42)"}"#,
        )
        .expect("legacy gpu payload should still parse");
        assert!(gpu.unmanaged_processes.is_empty());
    }

    #[test]
    fn scheduler_info_deserializes_without_ignore_overrides() {
        let info: SchedulerInfo = serde_json::from_str(
            r#"{"gpus":[],"allowed_gpu_indices":null,"gpu_allocation_strategy":"sequential"}"#,
        )
        .expect("legacy scheduler info should still parse");
        assert!(info.ignored_gpu_processes.is_empty());
    }

    #[test]
    fn idle_leftover_requires_zero_utilization_and_age() {
        let idle_long = UnmanagedGpuProcess {
            pid: 1,
            used_memory_mb: Some(642),
            utilization_percent: Some(0),
            age_secs: Some(2 * 24 * 3600),
        };
        assert!(idle_long.is_idle_leftover());

        // Busy process: not a leftover even if old.
        let busy = UnmanagedGpuProcess {
            pid: 2,
            used_memory_mb: Some(80_000),
            utilization_percent: Some(99),
            age_secs: Some(2 * 24 * 3600),
        };
        assert!(!busy.is_idle_leftover());

        // Fresh process: give it the benefit of the doubt.
        let fresh = UnmanagedGpuProcess {
            pid: 3,
            used_memory_mb: Some(100),
            utilization_percent: Some(0),
            age_secs: Some(60),
        };
        assert!(!fresh.is_idle_leftover());

        // Unknown utilization must not be treated as zero.
        let unknown = UnmanagedGpuProcess {
            pid: 4,
            used_memory_mb: Some(100),
            utilization_percent: None,
            age_secs: Some(10 * 24 * 3600),
        };
        assert!(!unknown.is_idle_leftover());

        // Unknown age must not be treated as old.
        let unknown_age = UnmanagedGpuProcess {
            pid: 5,
            used_memory_mb: Some(100),
            utilization_percent: Some(0),
            age_secs: None,
        };
        assert!(!unknown_age.is_idle_leftover());
    }

    #[test]
    fn release_command_matches_the_gctl_cli() {
        let process = UnmanagedGpuProcess {
            pid: 3471817,
            used_memory_mb: Some(642),
            utilization_percent: Some(0),
            age_secs: Some(2 * 24 * 3600),
        };
        assert_eq!(
            process.release_command(0),
            "gctl gpu-process ignore --gpu 0 --pid 3471817"
        );
    }

    #[test]
    fn prints_unmanaged_detail_and_release_hint() {
        let mut blocked = gpu(0, false, Some("unmanaged(pid=3471817)"));
        blocked.unmanaged_processes = vec![UnmanagedGpuProcess {
            pid: 3471817,
            used_memory_mb: Some(642),
            utilization_percent: Some(0),
            age_secs: Some(2 * 24 * 3600),
        }];
        let mut info = scheduler_info(vec![blocked, gpu(1, true, None)]);
        info.ignored_gpu_processes = vec![IgnoredGpuProcess {
            gpu_index: 2,
            pid: 999,
        }];

        // Rendering must not panic and must mention the actionable bits.
        print_gpu_allocation(&info, &[]);
    }
}
