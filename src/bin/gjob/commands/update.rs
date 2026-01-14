use anyhow::{anyhow, Result};
use gflow::client::{Client, UpdateJobRequest};
use gflow::utils::parse_job_ids;
use std::collections::HashMap;

pub struct UpdateJobParams {
    pub job_ids_str: String,
    pub command: Option<String>,
    pub script: Option<std::path::PathBuf>,
    pub gpus: Option<u32>,
    pub conda_env: Option<String>,
    pub clear_conda_env: bool,
    pub priority: Option<u8>,
    pub time_limit: Option<String>,
    pub clear_time_limit: bool,
    pub memory_limit: Option<String>,
    pub clear_memory_limit: bool,
    pub depends_on: Option<Vec<u32>>,
    pub depends_on_all: Option<Vec<u32>>,
    pub depends_on_any: Option<Vec<u32>>,
    pub auto_cancel_on_dep_failure: bool,
    pub no_auto_cancel_on_dep_failure: bool,
    pub max_concurrent: Option<usize>,
    pub clear_max_concurrent: bool,
    pub params: Vec<String>,
}

pub async fn handle_update(
    config_path: &Option<std::path::PathBuf>,
    params: UpdateJobParams,
) -> Result<()> {
    // Load config and create client
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    let job_ids = parse_job_ids(&params.job_ids_str)?;

    // Check that at least one update flag is provided
    let has_updates = params.command.is_some()
        || params.script.is_some()
        || params.gpus.is_some()
        || params.conda_env.is_some()
        || params.clear_conda_env
        || params.priority.is_some()
        || params.time_limit.is_some()
        || params.clear_time_limit
        || params.memory_limit.is_some()
        || params.clear_memory_limit
        || params.depends_on.is_some()
        || params.depends_on_all.is_some()
        || params.depends_on_any.is_some()
        || params.auto_cancel_on_dep_failure
        || params.no_auto_cancel_on_dep_failure
        || params.max_concurrent.is_some()
        || params.clear_max_concurrent
        || !params.params.is_empty();

    if !has_updates {
        return Err(anyhow!(
            "No updates specified. Use --help to see available options."
        ));
    }

    // Parse parameters
    let parameters = if !params.params.is_empty() {
        let mut param_map = HashMap::new();
        for param in &params.params {
            let parts: Vec<&str> = param.splitn(2, '=').collect();
            if parts.len() != 2 {
                return Err(anyhow!(
                    "Invalid parameter format '{}'. Expected KEY=VALUE",
                    param
                ));
            }
            param_map.insert(parts[0].to_string(), parts[1].to_string());
        }
        Some(param_map)
    } else {
        None
    };

    // Parse time limit
    let parsed_time_limit = if let Some(time_str) = &params.time_limit {
        Some(Some(gflow::utils::parse_time_limit(time_str)?))
    } else if params.clear_time_limit {
        Some(None)
    } else {
        None
    };

    // Parse memory limit
    let parsed_memory_limit = if let Some(mem_str) = &params.memory_limit {
        Some(Some(gflow::utils::parse_memory_limit(mem_str)?))
    } else if params.clear_memory_limit {
        Some(None)
    } else {
        None
    };

    // Handle conda_env
    let parsed_conda_env = if let Some(env) = &params.conda_env {
        Some(Some(env.clone()))
    } else if params.clear_conda_env {
        Some(None)
    } else {
        None
    };

    // Handle max_concurrent
    let parsed_max_concurrent = if let Some(mc) = params.max_concurrent {
        Some(Some(mc))
    } else if params.clear_max_concurrent {
        Some(None)
    } else {
        None
    };

    // Handle dependencies
    let (parsed_depends_on_ids, parsed_dependency_mode) = if let Some(deps) = &params.depends_on_all
    {
        (
            Some(deps.clone()),
            Some(Some(gflow::core::job::DependencyMode::All)),
        )
    } else if let Some(deps) = &params.depends_on_any {
        (
            Some(deps.clone()),
            Some(Some(gflow::core::job::DependencyMode::Any)),
        )
    } else if let Some(deps) = &params.depends_on {
        // Default to All mode if not specified
        (
            Some(deps.clone()),
            Some(Some(gflow::core::job::DependencyMode::All)),
        )
    } else {
        (None, None)
    };

    // Handle auto_cancel_on_dependency_failure
    let parsed_auto_cancel = if params.auto_cancel_on_dep_failure {
        Some(true)
    } else if params.no_auto_cancel_on_dep_failure {
        Some(false)
    } else {
        None
    };

    for &job_id in &job_ids {
        // Build update request
        let request = UpdateJobRequest {
            command: params.command.clone(),
            script: params.script.clone(),
            gpus: params.gpus,
            conda_env: parsed_conda_env.clone(),
            priority: params.priority,
            parameters: parameters.clone(),
            time_limit: parsed_time_limit,
            memory_limit_mb: parsed_memory_limit,
            depends_on_ids: parsed_depends_on_ids.clone(),
            dependency_mode: parsed_dependency_mode,
            auto_cancel_on_dependency_failure: parsed_auto_cancel,
            max_concurrent: parsed_max_concurrent,
        };

        // Update the job
        match client.update_job(job_id, request).await {
            Ok(response) => {
                println!("Job {} updated successfully.", job_id);
                if !response.updated_fields.is_empty() {
                    println!("  Updated fields: {}", response.updated_fields.join(", "));
                }
            }
            Err(e) => {
                eprintln!("Error updating job {}: {}", job_id, e);
            }
        }
    }

    Ok(())
}
