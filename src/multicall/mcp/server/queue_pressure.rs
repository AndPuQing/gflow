use gflow::core::info::SchedulerInfo;
use gflow::core::job::{Job, JobState};
use gflow::core::reservation::{GpuReservation, ReservationStatus};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::schemas::{GpuAvailabilityOutput, QueuePressureGroupOutput, QueuePressureOutput};

#[derive(Debug, Default)]
struct QueuePressureGroupAccumulator {
    queued: usize,
    running: usize,
    requested_gpus: u32,
}

pub(super) fn build_queue_pressure_output(
    info: SchedulerInfo,
    jobs: Vec<Job>,
    reservations: Vec<GpuReservation>,
) -> QueuePressureOutput {
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let available_gpus = info
        .gpus
        .iter()
        .filter(|gpu| gpu.available)
        .map(|gpu| gpu.index)
        .collect::<Vec<_>>();
    let unavailable_gpus = info
        .gpus
        .iter()
        .filter(|gpu| !gpu.available)
        .map(|gpu| GpuAvailabilityOutput {
            index: gpu.index,
            reason: gpu.reason.clone(),
        })
        .collect::<Vec<_>>();

    let mut running_jobs = 0usize;
    let mut queued_jobs = 0usize;
    let mut held_jobs = 0usize;
    let mut queued_requested_gpus = 0u32;
    let mut running_allocated_gpus = 0u32;
    let mut blocked_reasons = BTreeMap::new();
    let mut users = BTreeMap::<String, QueuePressureGroupAccumulator>::new();
    let mut projects = BTreeMap::<String, QueuePressureGroupAccumulator>::new();

    for job in &jobs {
        match job.state {
            JobState::Running => {
                running_jobs += 1;
                running_allocated_gpus += job
                    .gpu_ids
                    .as_ref()
                    .map(|ids| ids.len() as u32)
                    .unwrap_or(job.gpus);
                accumulate_queue_group(&mut users, job.submitted_by.as_ref(), job, false);
                if let Some(project) = &job.project {
                    accumulate_queue_group(&mut projects, project.as_ref(), job, false);
                }
            }
            JobState::Queued => {
                queued_jobs += 1;
                queued_requested_gpus = queued_requested_gpus.saturating_add(job.gpus);
                *blocked_reasons.entry(job_reason_label(job)).or_insert(0) += 1;
                accumulate_queue_group(&mut users, job.submitted_by.as_ref(), job, true);
                if let Some(project) = &job.project {
                    accumulate_queue_group(&mut projects, project.as_ref(), job, true);
                }
            }
            JobState::Hold => {
                held_jobs += 1;
                *blocked_reasons.entry(job_reason_label(job)).or_insert(0) += 1;
                accumulate_queue_group(&mut users, job.submitted_by.as_ref(), job, true);
                if let Some(project) = &job.project {
                    accumulate_queue_group(&mut projects, project.as_ref(), job, true);
                }
            }
            _ => {}
        }
    }

    let now = SystemTime::now();
    let reservations_active = reservations
        .iter()
        .filter(|reservation| {
            reservation.status == ReservationStatus::Active || reservation.is_active(now)
        })
        .count();

    QueuePressureOutput {
        generated_at,
        total_gpus: info.gpus.len(),
        available_gpus,
        unavailable_gpus,
        running_jobs,
        queued_jobs,
        held_jobs,
        queued_requested_gpus,
        running_allocated_gpus,
        blocked_reasons,
        users: queue_group_outputs(users),
        projects: queue_group_outputs(projects),
        reservations_total: reservations.len(),
        reservations_active,
    }
}

fn accumulate_queue_group(
    groups: &mut BTreeMap<String, QueuePressureGroupAccumulator>,
    name: &str,
    job: &Job,
    queued: bool,
) {
    let group = groups.entry(name.to_string()).or_default();
    if queued {
        group.queued += 1;
        group.requested_gpus = group.requested_gpus.saturating_add(job.gpus);
    } else {
        group.running += 1;
    }
}

fn queue_group_outputs(
    groups: BTreeMap<String, QueuePressureGroupAccumulator>,
) -> Vec<QueuePressureGroupOutput> {
    let mut outputs = groups
        .into_iter()
        .map(|(name, group)| QueuePressureGroupOutput {
            name,
            queued: group.queued,
            running: group.running,
            requested_gpus: group.requested_gpus,
        })
        .collect::<Vec<_>>();
    outputs.sort_by(|left, right| {
        right
            .requested_gpus
            .cmp(&left.requested_gpus)
            .then_with(|| right.queued.cmp(&left.queued))
            .then_with(|| left.name.cmp(&right.name))
    });
    outputs.truncate(10);
    outputs
}

fn job_reason_label(job: &Job) -> String {
    if let Some(reason) = job.reason.as_deref() {
        return reason.to_string();
    }

    match job.state {
        JobState::Hold => "JobHeldUser".to_string(),
        JobState::Queued => "Resources".to_string(),
        _ => "unknown".to_string(),
    }
}
