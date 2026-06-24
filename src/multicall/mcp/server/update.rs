use compact_str::CompactString;
use gflow::client::UpdateJobRequest;
use gflow::core::job::{GpuSharingMode, Job, JobState};
use std::path::PathBuf;
use std::time::Duration;

use super::helpers::serialize_job_value;
use super::schemas::{PreviewUpdateJobOutput, UpdateJobToolRequest};
use super::submit::resolve_job_notifications;

pub(super) fn build_update_request(
    params: UpdateJobToolRequest,
) -> Result<UpdateJobRequest, String> {
    if params.conda_env.is_some() && params.clear_conda_env.unwrap_or(false) {
        return Err("Cannot set and clear conda_env in the same request".to_string());
    }
    if params.time_limit_secs.is_some() && params.clear_time_limit.unwrap_or(false) {
        return Err("Cannot set and clear time_limit in the same request".to_string());
    }
    if params.memory_limit_mb.is_some() && params.clear_memory_limit.unwrap_or(false) {
        return Err("Cannot set and clear memory_limit_mb in the same request".to_string());
    }
    if params.gpu_memory_limit_mb.is_some() && params.clear_gpu_memory_limit.unwrap_or(false) {
        return Err("Cannot set and clear gpu_memory_limit_mb in the same request".to_string());
    }
    if params.dependency_mode.is_some() && params.clear_dependency_mode.unwrap_or(false) {
        return Err("Cannot set and clear dependency_mode in the same request".to_string());
    }
    if params.max_concurrent.is_some() && params.clear_max_concurrent.unwrap_or(false) {
        return Err("Cannot set and clear max_concurrent in the same request".to_string());
    }
    if params.max_retries.is_some() && params.clear_max_retries.unwrap_or(false) {
        return Err("Cannot set and clear max_retries in the same request".to_string());
    }

    let notifications =
        resolve_job_notifications(params.notify_email, params.notify_on, "update_job")?;

    Ok(UpdateJobRequest {
        command: params.command,
        script: params.script.map(PathBuf::from),
        gpus: params.gpus,
        conda_env: match (params.conda_env, params.clear_conda_env.unwrap_or(false)) {
            (Some(env), false) => Some(Some(env)),
            (None, true) => Some(None),
            _ => None,
        },
        priority: params.priority,
        parameters: params.parameters,
        time_limit: match (
            params.time_limit_secs,
            params.clear_time_limit.unwrap_or(false),
        ) {
            (Some(secs), false) => Some(Some(Duration::from_secs(secs))),
            (None, true) => Some(None),
            _ => None,
        },
        memory_limit_mb: match (
            params.memory_limit_mb,
            params.clear_memory_limit.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        gpu_memory_limit_mb: match (
            params.gpu_memory_limit_mb,
            params.clear_gpu_memory_limit.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        depends_on_ids: params.depends_on_ids,
        dependency_mode: match (
            params.dependency_mode,
            params.clear_dependency_mode.unwrap_or(false),
        ) {
            (Some(mode), false) => Some(Some(mode.into())),
            (None, true) => Some(None),
            _ => None,
        },
        auto_cancel_on_dependency_failure: params.auto_cancel_on_dependency_failure,
        max_concurrent: match (
            params.max_concurrent,
            params.clear_max_concurrent.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        max_retries: match (
            params.max_retries,
            params.clear_max_retries.unwrap_or(false),
        ) {
            (Some(value), false) => Some(Some(value)),
            (None, true) => Some(None),
            _ => None,
        },
        notifications,
    })
}

pub(super) fn preview_update_job_output(
    job: Job,
    request: UpdateJobRequest,
) -> PreviewUpdateJobOutput {
    let before = serialize_job_value(&job);
    let job_id = job.id;
    let mut warnings = vec![
        "dry run does not validate dependency existence or circular dependency changes".to_string(),
    ];

    if !matches!(job.state, JobState::Queued | JobState::Hold) {
        return PreviewUpdateJobOutput {
            dry_run: true,
            ok: false,
            job_id,
            before: Some(before),
            after: None,
            updated_fields: Vec::new(),
            error: Some(format!(
                "Job {} is in state '{}' and cannot be updated. Only queued or held jobs can be updated.",
                job_id, job.state
            )),
            warnings,
        };
    }

    if job.gpu_sharing_mode == GpuSharingMode::Shared
        && matches!(request.gpu_memory_limit_mb, Some(None))
    {
        return PreviewUpdateJobOutput {
            dry_run: true,
            ok: false,
            job_id,
            before: Some(before),
            after: None,
            updated_fields: Vec::new(),
            error: Some(
                "Shared jobs must keep a GPU memory limit (--gpu-memory / --max-gpu-mem)."
                    .to_string(),
            ),
            warnings,
        };
    }

    let (after_job, updated_fields) = apply_update_preview(job, request);
    if !updated_fields
        .iter()
        .any(|field| matches!(field.as_str(), "depends_on_ids" | "dependency_mode"))
    {
        warnings.clear();
    }

    PreviewUpdateJobOutput {
        dry_run: true,
        ok: true,
        job_id,
        before: Some(before),
        after: Some(serialize_job_value(&after_job)),
        updated_fields,
        error: None,
        warnings,
    }
}

fn apply_update_preview(mut job: Job, request: UpdateJobRequest) -> (Job, Vec<String>) {
    let mut updated_fields = Vec::new();

    if let Some(command) = request.command {
        job.command = Some(CompactString::from(command));
        updated_fields.push("command".to_string());
    }
    if let Some(script) = request.script {
        job.script = Some(Box::new(script));
        updated_fields.push("script".to_string());
    }
    if let Some(gpus) = request.gpus {
        job.gpus = gpus;
        updated_fields.push("gpus".to_string());
    }
    if let Some(conda_env) = request.conda_env {
        job.conda_env = conda_env.map(CompactString::from);
        updated_fields.push("conda_env".to_string());
    }
    if let Some(priority) = request.priority {
        job.priority = priority;
        updated_fields.push("priority".to_string());
    }
    if let Some(parameters) = request.parameters {
        job.parameters = parameters
            .into_iter()
            .map(|(key, value)| (CompactString::from(key), CompactString::from(value)))
            .collect();
        updated_fields.push("parameters".to_string());
    }
    if let Some(time_limit) = request.time_limit {
        job.time_limit = time_limit;
        updated_fields.push("time_limit".to_string());
    }
    if let Some(memory_limit_mb) = request.memory_limit_mb {
        job.memory_limit_mb = memory_limit_mb;
        updated_fields.push("memory_limit_mb".to_string());
    }
    if let Some(gpu_memory_limit_mb) = request.gpu_memory_limit_mb {
        job.gpu_memory_limit_mb = gpu_memory_limit_mb;
        updated_fields.push("gpu_memory_limit_mb".to_string());
    }
    if let Some(depends_on_ids) = request.depends_on_ids {
        job.depends_on_ids = depends_on_ids.into();
        updated_fields.push("depends_on_ids".to_string());
    }
    if let Some(dependency_mode) = request.dependency_mode {
        job.dependency_mode = dependency_mode;
        updated_fields.push("dependency_mode".to_string());
    }
    if let Some(auto_cancel) = request.auto_cancel_on_dependency_failure {
        job.auto_cancel_on_dependency_failure = auto_cancel;
        updated_fields.push("auto_cancel_on_dependency_failure".to_string());
    }
    if let Some(max_concurrent) = request.max_concurrent {
        job.max_concurrent = max_concurrent;
        updated_fields.push("max_concurrent".to_string());
    }
    if let Some(max_retries) = request.max_retries {
        job.max_retries = max_retries.unwrap_or(0);
        updated_fields.push("max_retries".to_string());
    }
    if let Some(notifications) = request.notifications {
        job.notifications = notifications;
        updated_fields.push("notifications".to_string());
    }

    (job, updated_fields)
}
