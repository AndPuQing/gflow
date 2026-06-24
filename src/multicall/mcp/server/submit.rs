use gflow::core::job::{Job, JobBuilder, JobNotifications};
use gflow::utils::{generate_param_combinations, parse_param_spec};
use lettre::message::Mailbox;
use std::time::Duration;

use super::helpers::serialize_job_value;
use super::schemas::{PreviewSubmitJobOutput, PreviewSubmitJobResultOutput, SubmitJobRequest};

pub(super) fn build_submit_job(params: SubmitJobRequest) -> anyhow::Result<Job, String> {
    if params.command.is_none() && params.script.is_none() {
        return Err("submit_job requires either 'command' or 'script'".to_string());
    }
    if params.command.is_some() && params.script.is_some() {
        return Err("submit_job accepts either 'command' or 'script', not both".to_string());
    }
    if params.shared.unwrap_or(false) && params.gpu_memory_limit_mb.is_none() {
        return Err("submit_job requires 'gpu_memory_limit_mb' when 'shared' is true".to_string());
    }

    let mut builder = JobBuilder::new()
        .gpus(params.gpus.unwrap_or(0))
        .run_dir(
            params
                .run_dir
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into())),
        )
        .priority(params.priority.unwrap_or(10))
        .submitted_by(
            params
                .submitted_by
                .unwrap_or_else(resolve_default_submitted_by),
        )
        .auto_close_tmux(params.auto_close_tmux.unwrap_or(false))
        .shared(params.shared.unwrap_or(false))
        .max_concurrent(params.max_concurrent)
        .max_retries(params.max_retries.unwrap_or(0))
        .run_name(params.run_name)
        .project(params.project);

    if let Some(notifications) =
        resolve_job_notifications(params.notify_email, params.notify_on, "submit_job")?
    {
        builder = builder.notifications(notifications);
    }

    if let Some(command) = params.command {
        builder = builder.command(command);
    }
    if let Some(script) = params.script {
        builder = builder.script(script);
    }
    if let Some(conda_env) = params.conda_env {
        builder = builder.conda_env(Some(conda_env));
    }
    if let Some(depends_on) = params.depends_on {
        builder = builder.depends_on(Some(depends_on));
    }
    if let Some(depends_on_ids) = params.depends_on_ids {
        builder = builder.depends_on_ids(depends_on_ids);
    }
    if let Some(dependency_mode) = params.dependency_mode {
        builder = builder.dependency_mode(Some(dependency_mode.into()));
    }
    if let Some(auto_cancel) = params.auto_cancel_on_dependency_failure {
        builder = builder.auto_cancel_on_dependency_failure(auto_cancel);
    }
    if let Some(gpu_memory_limit_mb) = params.gpu_memory_limit_mb {
        builder = builder.gpu_memory_limit_mb(Some(gpu_memory_limit_mb));
    }
    if let Some(time_limit_secs) = params.time_limit_secs {
        builder = builder.time_limit(Some(Duration::from_secs(time_limit_secs)));
    }
    if let Some(memory_limit_mb) = params.memory_limit_mb {
        builder = builder.memory_limit_mb(Some(memory_limit_mb));
    }
    if let Some(parameters) = params.parameters {
        builder = builder.parameters(parameters);
    }

    Ok(builder.build())
}

pub(super) fn expand_submit_job_requests(
    jobs: Vec<SubmitJobRequest>,
) -> Result<Vec<(usize, SubmitJobRequest)>, String> {
    let mut expanded = Vec::new();

    for (index, job) in jobs.into_iter().enumerate() {
        expanded.extend(expand_single_submit_job_request(index, job)?);
    }

    Ok(expanded)
}

fn expand_single_submit_job_request(
    index: usize,
    job: SubmitJobRequest,
) -> Result<Vec<(usize, SubmitJobRequest)>, String> {
    let Some(param_specs_raw) = job.param.clone().filter(|params| !params.is_empty()) else {
        return Ok(vec![(index, job)]);
    };

    let mut parsed_specs = Vec::with_capacity(param_specs_raw.len());
    for spec in &param_specs_raw {
        parsed_specs.push(parse_param_spec(spec).map_err(|err| err.to_string())?);
    }

    let param_combinations = generate_param_combinations(&parsed_specs);
    let mut expanded_jobs = Vec::with_capacity(param_combinations.len());

    for combination in param_combinations {
        let mut expanded_job = job.clone();
        expanded_job.param = None;

        let mut parameters = expanded_job.parameters.take().unwrap_or_default();
        for (key, value) in combination {
            if parameters.contains_key(&key) {
                return Err(format!(
                    "submit_job cannot use the same key in both 'parameters' and 'param': {}",
                    key
                ));
            }
            parameters.insert(key, value);
        }

        expanded_job.parameters = if parameters.is_empty() {
            None
        } else {
            Some(parameters)
        };
        expanded_jobs.push((index, expanded_job));
    }

    Ok(expanded_jobs)
}

pub(super) fn preview_submit_jobs_output(
    jobs: Vec<SubmitJobRequest>,
    input_count: usize,
) -> PreviewSubmitJobOutput {
    let warnings = vec![
        "dry run only validates MCP-side request shape; daemon-side dependency, cycle, and project policy checks still run at submission time".to_string(),
    ];

    let expanded_jobs = match expand_submit_job_requests(jobs) {
        Ok(expanded_jobs) => expanded_jobs,
        Err(error) => {
            return PreviewSubmitJobOutput {
                dry_run: true,
                valid: false,
                input_count,
                expanded_count: 0,
                jobs: vec![PreviewSubmitJobResultOutput {
                    input_index: 0,
                    expanded_index: 0,
                    ok: false,
                    job: None,
                    error: Some(error),
                    warnings: Vec::new(),
                }],
                warnings,
            };
        }
    };

    let mut results = Vec::with_capacity(expanded_jobs.len());
    for (expanded_index, (input_index, params)) in expanded_jobs.into_iter().enumerate() {
        match build_submit_job(params) {
            Ok(job) => {
                let job_warnings = preview_submit_warnings(&job);
                results.push(PreviewSubmitJobResultOutput {
                    input_index,
                    expanded_index,
                    ok: true,
                    job: Some(serialize_job_value(&job)),
                    error: None,
                    warnings: job_warnings,
                });
            }
            Err(error) => {
                results.push(PreviewSubmitJobResultOutput {
                    input_index,
                    expanded_index,
                    ok: false,
                    job: None,
                    error: Some(error),
                    warnings: Vec::new(),
                });
            }
        }
    }

    let valid = results.iter().all(|result| result.ok);
    PreviewSubmitJobOutput {
        dry_run: true,
        valid,
        input_count,
        expanded_count: results.len(),
        jobs: results,
        warnings,
    }
}

fn preview_submit_warnings(job: &Job) -> Vec<String> {
    let mut warnings = Vec::new();
    if job.depends_on.is_some() || !job.depends_on_ids.is_empty() {
        warnings.push(
            "dependency existence and circular dependency checks require the daemon submit path"
                .to_string(),
        );
    }
    if job.project.is_some() {
        warnings.push("project policy validation requires the daemon submit path".to_string());
    }
    warnings
}

pub(super) fn resolve_job_notifications(
    notify_email: Option<Vec<String>>,
    notify_on: Option<Vec<String>>,
    context: &str,
) -> Result<Option<JobNotifications>, String> {
    let Some(emails) = notify_email else {
        if notify_on.is_some() {
            return Err(format!(
                "{context} requires 'notify_email' when 'notify_on' is set"
            ));
        }
        return Ok(None);
    };

    for email in &emails {
        email.parse::<Mailbox>().map_err(|err| {
            format!(
                "{context} received invalid email recipient '{}': {err}",
                email
            )
        })?;
    }

    if emails.is_empty() && notify_on.as_ref().is_some_and(|events| !events.is_empty()) {
        return Err(format!(
            "{context} cannot use 'notify_on' with an empty 'notify_email' list"
        ));
    }

    Ok(Some(JobNotifications::normalized(
        emails,
        notify_on.unwrap_or_default(),
    )))
}

fn resolve_default_submitted_by() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
