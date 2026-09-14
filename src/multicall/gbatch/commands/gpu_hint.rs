//! Post-submission advisory about GPUs that are blocked by non-gflow
//! processes.
//!
//! A stale process holding a few hundred MB with no kernels running makes the
//! scheduler treat a whole card as unusable, silently halving how much work
//! the cluster can run. The `gctl gpu-process ignore` escape hatch exists, but
//! it was effectively undiscoverable: nothing in the submission path
//! mentioned it. Print the blocked PIDs and the exact command that releases
//! them right after a submission, when the operator is actually looking.

use gflow::client::Client;

/// Print an advisory to stderr when GPUs are unavailable because of
/// non-gflow processes. Never fails the submission: any error here is
/// swallowed, since this is purely informational.
pub(crate) async fn warn_if_gpu_capacity_is_blocked(client: &Client) {
    let Ok(info) = client.get_info().await else {
        return;
    };

    let blocked: Vec<_> = info
        .gpus
        .iter()
        .filter(|gpu| !gpu.available && !gpu.unmanaged_processes.is_empty())
        .collect();

    if blocked.is_empty() {
        return;
    }

    let total = info.gpus.len();
    let unavailable = info.gpus.iter().filter(|gpu| !gpu.available).count();

    eprintln!();
    eprintln!(
        "Note: {} of {} GPUs are unavailable; {} blocked by non-gflow processes.",
        unavailable,
        total,
        blocked.len()
    );
    for gpu in &blocked {
        for process in &gpu.unmanaged_processes {
            let detail = describe(process);
            eprintln!(
                "  gpu {} pid {}{} -> release with: {}",
                gpu.index,
                process.pid,
                detail,
                process.release_command(gpu.index)
            );
        }
    }
    eprintln!("  (`ginfo` shows full detail; `gctl gpu-process list` shows active overrides)");
}

fn describe(process: &gflow::core::info::UnmanagedGpuProcess) -> String {
    let memory = process
        .used_memory_mb
        .map(gflow::utils::format_memory)
        .unwrap_or_else(|| "?".to_string());
    let util = process
        .utilization_percent
        .map(|value| format!("{}%", value))
        .unwrap_or_else(|| "?".to_string());
    let age = process
        .age_secs
        .map(|secs| gflow::utils::format_duration_compact(std::time::Duration::from_secs(secs)))
        .unwrap_or_else(|| "?".to_string());

    let note = if process.is_idle_leftover() {
        " [idle leftover]"
    } else {
        ""
    };
    format!(" ({memory}, util {util}, age {age}){note}")
}

#[cfg(test)]
mod tests {
    use gflow::core::info::UnmanagedGpuProcess;

    use super::*;

    #[test]
    fn describes_an_idle_leftover_with_its_metrics() {
        let process = UnmanagedGpuProcess {
            pid: 3471817,
            used_memory_mb: Some(642),
            utilization_percent: Some(0),
            age_secs: Some(2 * 24 * 3600),
        };

        assert_eq!(
            describe(&process),
            " (642M, util 0%, age 48h) [idle leftover]"
        );
    }

    #[test]
    fn describes_a_busy_process_without_the_leftover_note() {
        let process = UnmanagedGpuProcess {
            pid: 42,
            used_memory_mb: Some(80 * 1024),
            utilization_percent: Some(99),
            age_secs: Some(3 * 3600),
        };

        let rendered = describe(&process);
        assert!(rendered.contains("80G"), "{rendered}");
        assert!(rendered.contains("util 99%"), "{rendered}");
        assert!(!rendered.contains("idle leftover"), "{rendered}");
    }

    #[test]
    fn tolerates_missing_nvml_metrics() {
        let process = UnmanagedGpuProcess {
            pid: 7,
            used_memory_mb: None,
            utilization_percent: None,
            age_secs: None,
        };

        let rendered = describe(&process);
        assert!(rendered.contains("util ?"), "{rendered}");
        assert!(rendered.contains("age ?"), "{rendered}");
        // Unknown utilization must never be announced as an idle leftover.
        assert!(!rendered.contains("idle leftover"), "{rendered}");
    }
}
