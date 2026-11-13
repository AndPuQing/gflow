use super::*;
use gflow::config::GpuAdmissionConfig;

fn check_admission(
    nvml: Option<&Nvml>,
    index: u32,
    limits: GpuAdmissionConfig,
) -> Result<(), String> {
    let sample = || {
        let device = nvml
            .context("NVML is unavailable")?
            .device_by_index(index)?;
        let utilization = device.utilization_rates()?.gpu;
        let memory = device.memory_info()?;
        Ok::<_, anyhow::Error>((utilization, memory.used, memory.total))
    };
    let (utilization, used, total) =
        sample().map_err(|error| format!("gpu_admission_sample_failed: {error}"))?;
    check_sample(utilization, used, total, limits)
}

fn check_sample(
    utilization: u32,
    used: u64,
    total: u64,
    limits: GpuAdmissionConfig,
) -> Result<(), String> {
    if total == 0 {
        return Err("gpu_admission_sample_failed: zero total memory".into());
    }
    if utilization >= limits.max_utilization_percent
        || u128::from(used) * 100 >= u128::from(total) * u128::from(limits.max_memory_used_percent)
    {
        return Err(format!(
            "gpu_admission_threshold: utilization {utilization}% (limit {}%), memory {used}/{total} bytes (limit {}%)",
            limits.max_utilization_percent, limits.max_memory_used_percent
        ));
    }
    Ok(())
}

#[cfg(test)]
mod admission_tests {
    use super::*;

    #[test]
    fn strict_boundaries_and_missing_nvml() {
        let limits = GpuAdmissionConfig {
            max_utilization_percent: 50,
            max_memory_used_percent: 60,
        };
        assert!(check_sample(49, 5999, 10000, limits).is_ok());
        assert!(check_sample(50, 5999, 10000, limits).is_err());
        assert!(check_sample(49, 6000, 10000, limits).is_err());
        assert!(check_sample(0, 0, 0, limits).is_err());
        assert!(check_admission(None, 0, limits).is_err());
    }
}

impl SchedulerRuntime {
    pub fn set_gpu_admission(&mut self, limits: GpuAdmissionConfig) {
        self.gpu_admission = Some(limits);
    }

    pub(super) fn check_job_gpu_admission(&self, job: &Job) -> Result<(), String> {
        if let (Some(limits), Some(indices)) = (self.gpu_admission, &job.gpu_ids) {
            for &index in indices {
                check_admission(self.nvml.as_ref(), index, limits)?;
            }
        }
        Ok(())
    }

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

        if let Some(nvml) = &self.nvml {
            let ignored_snapshot = self.ignored_gpu_processes.clone();
            let mut active_ignored = ignored_snapshot.clone();
            if let Ok(device_count) = nvml.device_count() {
                for i in 0..device_count {
                    let Ok(device) = nvml.device_by_index(i) else {
                        tracing::warn!(gpu_index = i, "Failed to query NVML device by index");
                        continue;
                    };
                    let Ok(uuid) = device.uuid() else {
                        tracing::warn!(gpu_index = i, "Failed to query NVML device UUID");
                        continue;
                    };
                    let Some(slot) = self.scheduler.gpu_slots_mut().get_mut(&uuid) else {
                        continue;
                    };

                    let occupied_by_exclusive = running_exclusive_gpu_indices.contains(&slot.index);
                    let occupied_by_shared = running_shared_gpu_indices.contains(&slot.index);
                    let slot_index = slot.index;

                    match device.running_compute_processes() {
                        Ok(processes) => {
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

                            active_ignored.retain(|entry| entry.gpu_index != slot_index);
                            for pid in &ignored_pids {
                                active_ignored.insert(IgnoredGpuProcess {
                                    gpu_index: slot_index,
                                    pid: *pid,
                                });
                            }

                            unmanaged_pids.retain(|pid| {
                                !ignored_snapshot.contains(&IgnoredGpuProcess {
                                    gpu_index: slot_index,
                                    pid: *pid,
                                })
                            });
                            let is_free_in_nvml = unmanaged_pids.is_empty();
                            slot.available = if occupied_by_exclusive {
                                false
                            } else if occupied_by_shared {
                                true
                            } else {
                                is_free_in_nvml
                            };

                            if !occupied_by_exclusive && !occupied_by_shared {
                                if !is_free_in_nvml {
                                    slot.reason =
                                        Some(format_unmanaged_process_reason(&unmanaged_pids));
                                } else if !ignored_pids.is_empty() {
                                    slot.reason = Some(format_manual_ignore_reason(
                                        slot_index,
                                        &ignored_pids,
                                    ));
                                } else {
                                    slot.reason = None;
                                }
                            } else {
                                slot.reason = None;
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                gpu_index = slot_index,
                                error = ?error,
                                "Failed to inspect running GPU processes; keeping scheduler conservative"
                            );
                            slot.available = occupied_by_shared;
                            slot.reason = if occupied_by_exclusive || occupied_by_shared {
                                None
                            } else {
                                Some("nvml_query_failed".to_string())
                            };
                        }
                    }
                }
            } else {
                tracing::warn!("Failed to query NVML device count during GPU refresh");
            }
            self.ignored_gpu_processes = active_ignored;
        }
        if let Some(limits) = self.gpu_admission {
            for slot in self.scheduler.gpu_slots_mut().values_mut() {
                if let Err(reason) = check_admission(self.nvml.as_ref(), slot.index, limits) {
                    slot.available = false;
                    slot.reason = Some(reason);
                }
            }
        }
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
