use super::*;
use gflow::core::info::UnmanagedGpuProcess;
use nvml_wrapper::enums::device::UsedGpuMemory;

/// GPU memory held by a process, in MB, when NVML reports it.
fn process_memory_mb(memory: UsedGpuMemory) -> Option<u64> {
    match memory {
        UsedGpuMemory::Used(bytes) => Some(bytes / (1024 * 1024)),
        UsedGpuMemory::Unavailable => None,
    }
}

/// Outcome of inspecting one GPU during a refresh. Computed without holding a
/// borrow on the scheduler so the slot can be updated afterwards.
struct GpuProcessScan {
    unmanaged_pids: Vec<u32>,
    ignored_pids: Vec<u32>,
    processes: Vec<UnmanagedGpuProcess>,
    /// `Some` when NVML answered; `None` when the query failed.
    processes_ok: bool,
}

/// Inspect the compute processes on one device and classify them into
/// ignored-by-override vs. genuinely unmanaged.
///
/// Takes its mutable state as explicit arguments (rather than `&mut self`) so
/// callers can keep disjoint borrows of the runtime fields alive.
fn scan_gpu_processes(
    device: &nvml_wrapper::Device<'_>,
    slot_index: u32,
    ignored_snapshot: &HashSet<IgnoredGpuProcess>,
    active_ignored: &mut HashSet<IgnoredGpuProcess>,
    utilization_last_seen: &mut HashMap<u32, u64>,
) -> GpuProcessScan {
    let processes = match device.running_compute_processes() {
        Ok(processes) => processes,
        Err(_) => {
            return GpuProcessScan {
                unmanaged_pids: Vec::new(),
                ignored_pids: Vec::new(),
                processes: Vec::new(),
                processes_ok: false,
            }
        }
    };

    // Per-process SM utilization since the previous poll. `None` when the
    // driver does not support the query, which we must not conflate with
    // "utilization is zero".
    let utilization_by_pid = gpu_process_utilization(device, slot_index, utilization_last_seen);

    let mut memory_by_pid: HashMap<u32, Option<u64>> = HashMap::new();
    for process in &processes {
        memory_by_pid.insert(
            process.pid,
            process_memory_mb(process.used_gpu_memory.clone()),
        );
    }

    let mut unmanaged_pids = processes
        .into_iter()
        .map(|proc| proc.pid)
        .collect::<Vec<_>>();
    unmanaged_pids.sort_unstable();
    unmanaged_pids.dedup();

    let ignored_pids: Vec<u32> = unmanaged_pids
        .iter()
        .copied()
        .filter(|pid| {
            ignored_snapshot.contains(&IgnoredGpuProcess {
                gpu_index: slot_index,
                pid: *pid,
            })
        })
        .collect();

    // Drop overrides whose process is gone, so a stale entry cannot keep a
    // GPU marked as manually ignored forever.
    active_ignored.retain(|entry| entry.gpu_index != slot_index);
    for pid in &ignored_pids {
        active_ignored.insert(IgnoredGpuProcess {
            gpu_index: slot_index,
            pid: *pid,
        });
    }

    unmanaged_pids.retain(|pid| !ignored_pids.contains(pid));

    let processes = unmanaged_pids
        .iter()
        .map(|pid| UnmanagedGpuProcess {
            pid: *pid,
            used_memory_mb: memory_by_pid.get(pid).copied().flatten(),
            utilization_percent: utilization_by_pid
                .as_ref()
                .and_then(|map| map.get(pid).copied()),
            age_secs: gflow::platform::process_age_secs(*pid),
        })
        .collect();

    GpuProcessScan {
        unmanaged_pids,
        ignored_pids,
        processes,
        processes_ok: true,
    }
}

/// Per-process SM utilization on `device`, keyed by PID, over the interval
/// since the previous poll. `None` when the driver cannot report it.
///
/// NVML returns samples only for processes that did work in the interval, so a
/// PID missing from a successful query legitimately means "0%".
fn gpu_process_utilization(
    device: &nvml_wrapper::Device<'_>,
    slot_index: u32,
    utilization_last_seen: &mut HashMap<u32, u64>,
) -> Option<HashMap<u32, u32>> {
    let last_seen = utilization_last_seen.get(&slot_index).copied().unwrap_or(0);

    match device.process_utilization_stats(last_seen) {
        Ok(samples) => {
            if let Some(max_ts) = samples.iter().map(|sample| sample.timestamp).max() {
                utilization_last_seen.insert(slot_index, max_ts);
            }
            Some(
                samples
                    .into_iter()
                    .map(|sample| (sample.pid, sample.sm_util))
                    .collect(),
            )
        }
        Err(error) => {
            // Unsupported per-process utilization is the norm on many drivers
            // (and on CI without NVML), so this is trace-level: it must not
            // drown out real warnings on every 10s poll.
            tracing::trace!(
                gpu_index = slot_index,
                error = ?error,
                "Per-process GPU utilization unavailable"
            );
            None
        }
    }
}

impl SchedulerRuntime {
    pub(super) fn refresh_gpu_slots(&mut self) {
        let mut running_shared_gpu_indices = HashSet::new();
        let mut running_exclusive_gpu_indices = HashSet::new();

        for rt in self
            .scheduler
            .job_runtimes()
            .iter()
            .filter(|rt| rt.state == JobState::Running)
        {
            let Some(gpu_ids) = rt.gpu_ids.as_ref() else {
                continue;
            };

            match rt.gpu_sharing_mode {
                GpuSharingMode::Shared => {
                    for &gpu in gpu_ids {
                        running_shared_gpu_indices.insert(gpu);
                    }
                }
                GpuSharingMode::Exclusive => {
                    for &gpu in gpu_ids {
                        running_exclusive_gpu_indices.insert(gpu);
                    }
                }
            }
        }

        let Some(nvml) = self.nvml.as_ref() else {
            return;
        };

        let ignored_snapshot = self.ignored_gpu_processes.clone();
        let mut active_ignored = ignored_snapshot.clone();

        let Ok(device_count) = nvml.device_count() else {
            tracing::warn!("Failed to query NVML device count during GPU refresh");
            return;
        };

        for i in 0..device_count {
            let Ok(device) = nvml.device_by_index(i) else {
                tracing::warn!(gpu_index = i, "Failed to query NVML device by index");
                continue;
            };
            let Ok(uuid) = device.uuid() else {
                tracing::warn!(gpu_index = i, "Failed to query NVML device UUID");
                continue;
            };

            // Read the slot's index with a short-lived borrow, then release it
            // so the scan below can touch other runtime fields freely.
            let Some(slot_index) = self
                .scheduler
                .gpu_slots_mut()
                .get(&uuid)
                .map(|slot| slot.index)
            else {
                continue;
            };

            let occupied_by_exclusive = running_exclusive_gpu_indices.contains(&slot_index);
            let occupied_by_shared = running_shared_gpu_indices.contains(&slot_index);

            let scan = scan_gpu_processes(
                &device,
                slot_index,
                &ignored_snapshot,
                &mut active_ignored,
                &mut self.gpu_utilization_last_seen,
            );

            let Some(slot) = self.scheduler.gpu_slots_mut().get_mut(&uuid) else {
                continue;
            };

            if !scan.processes_ok {
                tracing::warn!(
                    gpu_index = slot_index,
                    "Failed to inspect running GPU processes; keeping scheduler conservative"
                );
                slot.available = occupied_by_shared;
                slot.reason = if occupied_by_exclusive || occupied_by_shared {
                    None
                } else {
                    Some("nvml_query_failed".to_string())
                };
                continue;
            }

            let is_free_in_nvml = scan.unmanaged_pids.is_empty();
            slot.available = if occupied_by_exclusive {
                false
            } else if occupied_by_shared {
                true
            } else {
                is_free_in_nvml
            };

            if is_free_in_nvml {
                self.unmanaged_gpu_processes.remove(&slot_index);
            } else {
                self.unmanaged_gpu_processes
                    .insert(slot_index, scan.processes.clone());
            }

            if occupied_by_exclusive || occupied_by_shared {
                slot.reason = None;
            } else if !is_free_in_nvml {
                slot.reason = Some(format_unmanaged_process_reason(&scan.unmanaged_pids));
            } else if !scan.ignored_pids.is_empty() {
                slot.reason = Some(format_manual_ignore_reason(slot_index, &scan.ignored_pids));
            } else {
                slot.reason = None;
            }
        }

        self.ignored_gpu_processes = active_ignored;
    }
    fn current_compute_processes_on_gpu(&self, gpu_index: u32) -> Result<Vec<u32>> {
        let nvml = self
            .nvml
            .as_ref()
            .context("NVML is unavailable; GPU process inspection is not supported")?;

        if !self.scheduler.has_gpu_index(gpu_index) {
            anyhow::bail!(
                "Invalid GPU index {} (scheduler does not manage that GPU)",
                gpu_index,
            );
        }

        let device = nvml
            .device_by_index(gpu_index)
            .with_context(|| format!("Failed to inspect GPU {}", gpu_index))?;
        let mut pids = device
            .running_compute_processes()
            .with_context(|| format!("Failed to inspect running processes on GPU {}", gpu_index))?
            .into_iter()
            .map(|proc| proc.pid)
            .collect::<Vec<_>>();
        pids.sort_unstable();
        pids.dedup();
        Ok(pids)
    }

    pub fn ignore_gpu_process(&mut self, gpu_index: u32, pid: u32) -> Result<bool> {
        let current_pids = self.current_compute_processes_on_gpu(gpu_index)?;
        if !current_pids.contains(&pid) {
            anyhow::bail!("PID {} is not currently running on GPU {}", pid, gpu_index);
        }

        let inserted = self
            .ignored_gpu_processes
            .insert(IgnoredGpuProcess { gpu_index, pid });
        self.refresh_gpu_slots();
        Ok(inserted)
    }

    pub fn unignore_gpu_process(&mut self, gpu_index: u32, pid: u32) -> bool {
        let removed = self
            .ignored_gpu_processes
            .remove(&IgnoredGpuProcess { gpu_index, pid });
        self.refresh_gpu_slots();
        removed
    }

    pub fn list_ignored_gpu_processes(&self) -> Vec<IgnoredGpuProcess> {
        let mut processes = self
            .ignored_gpu_processes
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        processes.sort_unstable();
        processes
    }
}

fn format_pid_list(pids: &[u32]) -> String {
    pids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn format_manual_ignore_reason(gpu_index: u32, ignored_pids: &[u32]) -> String {
    format!(
        "manual_ignore(gpu={},pid={})",
        gpu_index,
        format_pid_list(ignored_pids)
    )
}

pub(super) fn format_unmanaged_process_reason(unmanaged_pids: &[u32]) -> String {
    format!("unmanaged(pid={})", format_pid_list(unmanaged_pids))
}
