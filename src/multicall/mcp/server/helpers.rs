use gflow::core::info::SchedulerInfo;
use gflow::core::job::Job;
use rmcp::model::CallToolResult;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

use super::schemas::{
    GpuInfoOutput, IgnoredGpuProcessOutput, SchedulerInfoOutput, UnmanagedGpuProcessOutput,
};

/// Convert the daemon's scheduler info into the MCP output schema, resolving
/// each unmanaged process into actionable fields (idle-leftover verdict and the
/// exact release command).
pub(super) fn scheduler_info_output(info: SchedulerInfo) -> SchedulerInfoOutput {
    SchedulerInfoOutput {
        gpus: info
            .gpus
            .into_iter()
            .map(|gpu| GpuInfoOutput {
                uuid: gpu.uuid,
                index: gpu.index,
                available: gpu.available,
                reason: gpu.reason,
                unmanaged_processes: gpu
                    .unmanaged_processes
                    .iter()
                    .map(|process| UnmanagedGpuProcessOutput {
                        pid: process.pid,
                        used_memory_mb: process.used_memory_mb,
                        utilization_percent: process.utilization_percent,
                        age_secs: process.age_secs,
                        idle_leftover: process.is_idle_leftover(),
                        release_command: process.release_command(gpu.index),
                    })
                    .collect(),
            })
            .collect(),
        allowed_gpu_indices: info.allowed_gpu_indices,
        gpu_allocation_strategy: info.gpu_allocation_strategy.to_string(),
        ignored_gpu_processes: info
            .ignored_gpu_processes
            .into_iter()
            .map(|process| IgnoredGpuProcessOutput {
                gpu_index: process.gpu_index,
                pid: process.pid,
            })
            .collect(),
    }
}

pub(super) fn structured_response<T: serde::Serialize>(
    value: T,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let value = serde_json::to_value(value).map_err(|err| {
        rmcp::ErrorData::internal_error(format!("Failed to serialize MCP response: {}", err), None)
    })?;

    Ok(CallToolResult::structured(value))
}

pub(super) fn stringify_error(err: anyhow::Error) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(err.to_string(), None)
}

pub(super) fn serialize_job_value(job: &Job) -> Value {
    serde_json::to_value(job).unwrap_or_else(|err| {
        json!({
            "error": format!("Failed to serialize job: {}", err),
        })
    })
}

pub(super) fn system_time_to_unix_secs(ts: SystemTime) -> Option<u64> {
    ts.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}
