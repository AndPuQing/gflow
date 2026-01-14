use crate::cli;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use gflow::client::Client;
use gflow::core::job::Job;
use std::{collections::HashMap, env, fs, io::Read, path::PathBuf};

/// Substitute {param_name} patterns in command with actual values (for preview only)
fn preview_substitute(command: &str, parameters: &HashMap<String, String>) -> String {
    let mut result = command.to_string();
    for (param_name, value) in parameters {
        let pattern = format!("{{{}}}", param_name);
        result = result.replace(&pattern, value);
    }
    result
}

/// Substitute {param_name} patterns in template with actual values
fn substitute_template(template: &str, parameters: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (param_name, value) in parameters {
        let pattern = format!("{{{}}}", param_name);
        // Sanitize value for use in tmux session names (replace spaces with underscores)
        let sanitized_value = value.replace([' ', '/'], "_");
        result = result.replace(&pattern, &sanitized_value);
    }
    result
}

/// Parse range specification (start:stop or start:stop:step)
/// Returns a vector of stringified values
fn parse_range_spec(spec: &str) -> Result<Vec<String>> {
    let parts: Vec<&str> = spec.split(':').collect();

    match parts.len() {
        2 => {
            // start:stop (default step = 1)
            let start = parts[0].trim().parse::<f64>()?;
            let stop = parts[1].trim().parse::<f64>()?;
            let step = if start <= stop { 1.0 } else { -1.0 };
            generate_range(start, stop, step)
        }
        3 => {
            // start:stop:step
            let start = parts[0].trim().parse::<f64>()?;
            let stop = parts[1].trim().parse::<f64>()?;
            let step = parts[2].trim().parse::<f64>()?;

            if step == 0.0 {
                return Err(anyhow!("Step cannot be zero"));
            }

            generate_range(start, stop, step)
        }
        _ => Err(anyhow!(
            "Invalid range format. Expected 'start:stop' or 'start:stop:step'"
        )),
    }
}

/// Generate range values from start to stop with given step
fn generate_range(start: f64, stop: f64, step: f64) -> Result<Vec<String>> {
    if (step > 0.0 && start > stop) || (step < 0.0 && start < stop) {
        return Err(anyhow!("Step direction doesn't match start/stop"));
    }

    // Determine decimal places to use for formatting based on step
    let step_str = format!("{}", step.abs());
    let decimal_places = if let Some(dot_pos) = step_str.find('.') {
        let after_dot = &step_str[dot_pos + 1..];
        // Remove trailing zeros and count
        after_dot.trim_end_matches('0').len()
    } else {
        0
    };

    let mut values = Vec::new();
    let mut index = 0;

    // Use epsilon for float comparison
    const EPSILON: f64 = 1e-10;

    loop {
        // Calculate current value from index to avoid accumulation of floating point errors
        let current = start + step * (index as f64);

        // Check if we've exceeded the stop value
        if step > 0.0 && current > stop + EPSILON {
            break;
        }
        if step < 0.0 && current < stop - EPSILON {
            break;
        }

        // Round to avoid floating point precision issues
        let power = 10_f64.powi(decimal_places.max(10) as i32);
        let rounded = (current * power).round() / power;

        // Format as integer if it's a whole number, otherwise as float with appropriate precision
        let formatted = if (rounded - rounded.round()).abs() < EPSILON {
            format!("{}", rounded.round() as i64)
        } else if decimal_places > 0 {
            format!("{:.prec$}", rounded, prec = decimal_places)
        } else {
            format!("{}", rounded)
        };

        values.push(formatted);
        index += 1;

        // Safety limit
        if values.len() > 10000 {
            return Err(anyhow!("Range too large (max 10000 values)"));
        }
    }

    Ok(values)
}

/// Parse a single parameter spec like "name=val1,val2,val3"
/// Returns (param_name, vec_of_values)
fn parse_param_spec(spec: &str) -> Result<(String, Vec<String>)> {
    let parts: Vec<&str> = spec.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid param format. Expected 'name=val1,val2,...'"
        ));
    }

    let name = parts[0].trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("Parameter name cannot be empty"));
    }

    let value_spec = parts[1];

    // Detect range syntax (contains : but not just in the middle of values)
    // Check if it looks like a range (has exactly 1 or 2 colons)
    let colon_count = value_spec.matches(':').count();
    let values = if (1..=2).contains(&colon_count) && !value_spec.contains(',') {
        // Range syntax: start:stop or start:stop:step
        parse_range_spec(value_spec)?
    } else {
        // Comma-separated syntax
        value_spec
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect()
    };

    if values.is_empty() {
        return Err(anyhow!("Parameter must have at least one value"));
    }

    Ok((name, values))
}

/// Generate cartesian product of parameter values
/// Example: {lr: [0.01, 0.1], bs: [32, 64]} →
///   [{lr: "0.01", bs: "32"}, {lr: "0.01", bs: "64"}, ...]
fn generate_param_combinations(
    param_specs: &[(String, Vec<String>)],
) -> Vec<HashMap<String, String>> {
    if param_specs.is_empty() {
        return vec![HashMap::new()];
    }

    let mut combinations = vec![HashMap::new()];

    for (param_name, values) in param_specs {
        let mut new_combinations = Vec::new();
        for combo in &combinations {
            for value in values {
                let mut new_combo = combo.clone();
                new_combo.insert(param_name.clone(), value.clone());
                new_combinations.push(new_combo);
            }
        }
        combinations = new_combinations;
    }

    combinations
}

/// Parse parameter file (CSV format)
/// Returns a vector of parameter sets, one per row
fn parse_param_file(path: &PathBuf) -> Result<Vec<HashMap<String, String>>> {
    let mut reader = csv::Reader::from_path(path).context("Failed to read parameter file")?;

    let headers: Vec<String> = reader
        .headers()?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();

    if headers.is_empty() {
        return Err(anyhow!("CSV file must have a header row"));
    }

    let mut param_sets = Vec::new();
    for result in reader.records() {
        let record = result?;
        let mut params = HashMap::new();

        for (i, value) in record.iter().enumerate() {
            if let Some(header) = headers.get(i) {
                params.insert(header.clone(), value.trim().to_string());
            }
        }

        param_sets.push(params);
    }

    if param_sets.is_empty() {
        return Err(anyhow!("CSV file contains no data rows"));
    }

    Ok(param_sets)
}

pub(crate) async fn handle_add(
    config: &gflow::config::Config,
    add_args: cli::AddArgs,
    use_stdin: bool,
) -> Result<()> {
    let client = Client::build(config).context("Failed to build client")?;

    // Read stdin content if needed
    let stdin_content = if use_stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        if buffer.trim().is_empty() {
            anyhow::bail!("No content provided via stdin");
        }
        Some(buffer)
    } else {
        None
    };

    // Validation: --param and --array are mutually exclusive
    if !add_args.param.is_empty() && add_args.array.is_some() {
        anyhow::bail!("Cannot use both --param and --array together");
    }

    // Validation: --param-file and --array are mutually exclusive
    if add_args.param_file.is_some() && add_args.array.is_some() {
        anyhow::bail!("Cannot use both --param-file and --array together");
    }

    // Handle --param-file mode
    if let Some(ref param_file) = add_args.param_file {
        let mut param_combinations = parse_param_file(param_file)?;

        // If --param flags are also provided, merge them with cartesian product
        if !add_args.param.is_empty() {
            let mut param_specs = Vec::new();
            for spec in &add_args.param {
                param_specs.push(parse_param_spec(spec)?);
            }
            let cli_combinations = generate_param_combinations(&param_specs);

            // Cartesian product of file params with CLI params
            let mut merged = Vec::new();
            for file_params in &param_combinations {
                for cli_params in &cli_combinations {
                    let mut combined = file_params.clone();
                    combined.extend(cli_params.clone());
                    merged.push(combined);
                }
            }
            param_combinations = merged;
        }

        // Generate group_id if max_concurrent is specified
        let group_id = if add_args.max_concurrent.is_some() {
            Some(uuid::Uuid::new_v4().to_string())
        } else {
            None
        };

        // Dry-run mode: preview without submitting
        if add_args.dry_run {
            println!("Would submit {} batch job(s):", param_combinations.len());
            for (idx, params) in param_combinations.iter().enumerate() {
                let job = build_job_with_params(
                    add_args.clone(),
                    params.clone(),
                    &client,
                    stdin_content.as_ref(),
                )
                .await?;

                // Show preview
                let mut cmd = if let Some(c) = &job.command {
                    c.clone()
                } else if let Some(s) = &job.script {
                    s.to_string_lossy().to_string()
                } else {
                    String::new()
                };
                // Apply substitution for preview
                cmd = preview_substitute(&cmd, params);
                println!("  [{}] {} (GPUs: {})", idx + 1, cmd, job.gpus);
            }
            return Ok(());
        }

        // Build all jobs first
        let mut jobs = Vec::with_capacity(param_combinations.len());
        for params in param_combinations {
            let mut job =
                build_job_with_params(add_args.clone(), params, &client, stdin_content.as_ref())
                    .await?;
            // Assign group_id and max_concurrent if needed
            job.group_id = group_id.clone();
            job.max_concurrent = add_args.max_concurrent;
            jobs.push(job);
        }

        // Submit in batch
        let responses = client
            .add_jobs(jobs)
            .await
            .context("Failed to add batch jobs")?;

        // Show group_id if jobs are part of a group
        if let Some(ref gid) = group_id {
            println!(
                "Submitted {} batch jobs with group_id: {}",
                responses.len(),
                gid
            );
            println!(
                "  (You can update the limit with: gctl set-limit {} <N>)",
                responses[0].id
            );
        }

        for response in responses {
            println!(
                "Submitted batch job {} ({})",
                response.id, response.run_name
            );
        }

        return Ok(());
    }

    // Handle --param mode
    if !add_args.param.is_empty() {
        // Parse all parameter specs
        let mut param_specs = Vec::new();
        for spec in &add_args.param {
            param_specs.push(parse_param_spec(spec)?);
        }

        // Generate cartesian product
        let param_combinations = generate_param_combinations(&param_specs);

        // Generate group_id if max_concurrent is specified
        let group_id = if add_args.max_concurrent.is_some() {
            Some(uuid::Uuid::new_v4().to_string())
        } else {
            None
        };

        // Dry-run mode: preview without submitting
        if add_args.dry_run {
            println!("Would submit {} batch job(s):", param_combinations.len());
            for (idx, params) in param_combinations.iter().enumerate() {
                let job = build_job_with_params(
                    add_args.clone(),
                    params.clone(),
                    &client,
                    stdin_content.as_ref(),
                )
                .await?;

                // Show preview
                let mut cmd = if let Some(c) = &job.command {
                    c.clone()
                } else if let Some(s) = &job.script {
                    s.to_string_lossy().to_string()
                } else {
                    String::new()
                };
                // Apply substitution for preview
                cmd = preview_substitute(&cmd, params);
                println!("  [{}] {} (GPUs: {})", idx + 1, cmd, job.gpus);
            }
            return Ok(());
        }

        // Build all jobs first
        let mut jobs = Vec::with_capacity(param_combinations.len());
        for params in param_combinations {
            let mut job =
                build_job_with_params(add_args.clone(), params, &client, stdin_content.as_ref())
                    .await?;
            // Assign group_id and max_concurrent if needed
            job.group_id = group_id.clone();
            job.max_concurrent = add_args.max_concurrent;
            jobs.push(job);
        }

        // Submit in batch
        let responses = client
            .add_jobs(jobs)
            .await
            .context("Failed to add batch jobs")?;

        // Show group_id if jobs are part of a group
        if let Some(ref gid) = group_id {
            println!(
                "Submitted {} batch jobs with group_id: {}",
                responses.len(),
                gid
            );
            println!(
                "  (You can update the limit with: gctl set-limit {} <N>)",
                responses[0].id
            );
        }

        for response in responses {
            println!(
                "Submitted batch job {} ({})",
                response.id, response.run_name
            );
        }

        return Ok(());
    }

    // Existing --array mode
    if let Some(array_spec) = &add_args.array {
        let task_ids = parse_array_spec(array_spec)?;

        // Generate group_id if max_concurrent is specified
        let group_id = if add_args.max_concurrent.is_some() {
            Some(uuid::Uuid::new_v4().to_string())
        } else {
            None
        };

        // Dry-run mode for array jobs
        if add_args.dry_run {
            println!("Would submit {} array job(s):", task_ids.len());
            for (idx, task_id) in task_ids.iter().enumerate() {
                let job = build_job(
                    add_args.clone(),
                    Some(*task_id),
                    &client,
                    stdin_content.as_ref(),
                )
                .await?;

                let cmd = if let Some(c) = &job.command {
                    c.clone()
                } else if let Some(s) = &job.script {
                    s.to_string_lossy().to_string()
                } else {
                    String::new()
                };
                println!(
                    "  [{}] {} (GPUs: {}, task_id: {})",
                    idx + 1,
                    cmd,
                    job.gpus,
                    task_id
                );
            }
            return Ok(());
        }

        // Build all array jobs first
        let mut jobs = Vec::with_capacity(task_ids.len());
        for task_id in task_ids {
            let mut job = build_job(
                add_args.clone(),
                Some(task_id),
                &client,
                stdin_content.as_ref(),
            )
            .await?;
            // Assign group_id and max_concurrent if needed
            job.group_id = group_id.clone();
            job.max_concurrent = add_args.max_concurrent;
            jobs.push(job);
        }

        // Submit in batch
        let responses = client
            .add_jobs(jobs)
            .await
            .context("Failed to add batch jobs")?;

        // Show group_id if jobs are part of a group
        if let Some(ref gid) = group_id {
            println!(
                "Submitted {} batch jobs with group_id: {}",
                responses.len(),
                gid
            );
            println!(
                "  (You can update the limit with: gctl set-limit {} <N>)",
                responses[0].id
            );
        }

        for response in responses {
            println!(
                "Submitted batch job {} ({})",
                response.id, response.run_name
            );
        }
        return Ok(());
    }

    // Dry-run for non-param, non-array jobs
    if add_args.dry_run {
        let job = build_job(add_args, None, &client, stdin_content.as_ref()).await?;
        println!("Would submit 1 batch job:");
        let cmd = if let Some(c) = &job.command {
            c.clone()
        } else if let Some(s) = &job.script {
            s.to_string_lossy().to_string()
        } else {
            String::new()
        };
        println!("  [1] {} (GPUs: {})", cmd, job.gpus);
        return Ok(());
    }

    // Single job submission (existing logic)
    let job = build_job(add_args, None, &client, stdin_content.as_ref()).await?;
    let response = client.add_job(job).await.context("Failed to add job")?;
    println!(
        "Submitted batch job {} ({})",
        response.id, response.run_name
    );

    Ok(())
}

/// Detects the currently active conda environment from the environment variables
fn detect_current_conda_env() -> Option<String> {
    env::var("CONDA_DEFAULT_ENV")
        .ok()
        .filter(|env_name| !env_name.is_empty())
}

async fn build_job(
    args: cli::AddArgs,
    task_id: Option<u32>,
    client: &Client,
    stdin_content: Option<&String>,
) -> Result<Job> {
    let mut builder = Job::builder();
    let run_dir = std::env::current_dir().context("Failed to get current directory")?;
    builder = builder.run_dir(run_dir);
    builder = builder.task_id(task_id);

    // Get the username of the submitter
    let username = gflow::core::get_current_username();
    builder = builder.submitted_by(username);

    // Set custom run name if provided
    builder = builder.run_name(args.name.clone());

    // Parse time limit if provided
    let time_limit = if let Some(time_str) = &args.time {
        Some(gflow::utils::parse_time_limit(time_str)?)
    } else {
        None
    };

    // Parse memory limit if provided
    let memory_limit_mb = if let Some(memory_str) = &args.memory {
        Some(gflow::utils::parse_memory_limit(memory_str)?)
    } else {
        None
    };

    // Handle dependencies (mutually exclusive via clap)
    let (depends_on_ids, dependency_mode) = if let Some(ref deps_all) = args.depends_on_all {
        let ids = parse_dependency_list(deps_all, client).await?;
        (ids, Some(gflow::core::job::DependencyMode::All))
    } else if let Some(ref deps_any) = args.depends_on_any {
        let ids = parse_dependency_list(deps_any, client).await?;
        (ids, Some(gflow::core::job::DependencyMode::Any))
    } else if let Some(ref dep) = args.depends_on {
        // Legacy single dependency
        let dep_id = resolve_dependency(Some(dep.clone()), client).await?;
        if let Some(id) = dep_id {
            (vec![id], Some(gflow::core::job::DependencyMode::All))
        } else {
            (vec![], None)
        }
    } else {
        (vec![], None)
    };

    builder = builder.depends_on_ids(depends_on_ids.clone());
    builder = builder.dependency_mode(dependency_mode);
    builder = builder.auto_cancel_on_dependency_failure(!args.no_auto_cancel);

    // For backward compatibility, also set depends_on if there's exactly one dependency
    if depends_on_ids.len() == 1 {
        builder = builder.depends_on(Some(depends_on_ids[0]));
    }

    if let Some(content) = stdin_content {
        // Stdin mode - save content to a temporary script file
        let script_args = parse_script_content_for_args(content)?;
        let temp_script = save_stdin_to_temp_file(content)?;

        builder = builder.script(temp_script);
        builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
        builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
        builder = builder.conda_env(args.conda_env.or(script_args.conda_env));

        // CLI time limit takes precedence over script time limit
        let final_time_limit = if time_limit.is_some() {
            time_limit
        } else if let Some(script_time_str) = &script_args.time {
            Some(gflow::utils::parse_time_limit(script_time_str)?)
        } else {
            None
        };
        builder = builder.time_limit(final_time_limit);

        // CLI memory limit takes precedence over script memory limit
        let final_memory_limit = if memory_limit_mb.is_some() {
            memory_limit_mb
        } else if let Some(script_memory_str) = &script_args.memory {
            Some(gflow::utils::parse_memory_limit(script_memory_str)?)
        } else {
            None
        };
        builder = builder.memory_limit_mb(final_memory_limit);
    } else {
        // Determine if it's a script or command
        let is_script =
            args.script_or_command.len() == 1 && PathBuf::from(&args.script_or_command[0]).exists();

        if is_script {
            // Script mode
            let script_path = make_absolute_path(PathBuf::from(&args.script_or_command[0]))?;
            let script_args = parse_script_for_args(&script_path)?;

            builder = builder.script(script_path);
            builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
            builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
            builder = builder.conda_env(args.conda_env.or(script_args.conda_env));

            // CLI time limit takes precedence over script time limit
            let final_time_limit = if time_limit.is_some() {
                time_limit
            } else if let Some(script_time_str) = &script_args.time {
                Some(gflow::utils::parse_time_limit(script_time_str)?)
            } else {
                None
            };
            builder = builder.time_limit(final_time_limit);

            // CLI memory limit takes precedence over script memory limit
            let final_memory_limit = if memory_limit_mb.is_some() {
                memory_limit_mb
            } else if let Some(script_memory_str) = &script_args.memory {
                Some(gflow::utils::parse_memory_limit(script_memory_str)?)
            } else {
                None
            };
            builder = builder.memory_limit_mb(final_memory_limit);
        } else {
            // Command mode
            let command = args
                .script_or_command
                .iter()
                .map(|arg| shell_escape::escape(arg.into()))
                .collect::<Vec<_>>()
                .join(" ");
            builder = builder.command(command);
            builder = builder.gpus(args.gpus.unwrap_or(0));
            builder = builder.priority(args.priority.unwrap_or(10));

            // Auto-detect conda environment if not specified
            let conda_env = args.conda_env.or_else(detect_current_conda_env);
            builder = builder.conda_env(conda_env);

            builder = builder.time_limit(time_limit);
            builder = builder.memory_limit_mb(memory_limit_mb);
        }
    }

    // Set auto-close tmux flag
    builder = builder.auto_close_tmux(args.auto_close);

    Ok(builder.build())
}

async fn build_job_with_params(
    args: cli::AddArgs,
    parameters: HashMap<String, String>,
    client: &Client,
    stdin_content: Option<&String>,
) -> Result<Job> {
    let mut builder = Job::builder();
    let run_dir = std::env::current_dir().context("Failed to get current directory")?;
    builder = builder.run_dir(run_dir);
    // Parameters are for array-like submissions but without task_id
    builder = builder.task_id(None);
    builder = builder.parameters(parameters.clone());

    // Get the username of the submitter
    let username = gflow::core::get_current_username();
    builder = builder.submitted_by(username);

    // Apply name template if provided, otherwise use custom name if provided
    let run_name = if let Some(ref template) = args.name_template {
        Some(substitute_template(template, &parameters))
    } else {
        args.name.clone()
    };

    builder = builder.run_name(run_name);

    // Parse time limit if provided
    let time_limit = if let Some(time_str) = &args.time {
        Some(gflow::utils::parse_time_limit(time_str)?)
    } else {
        None
    };

    // Parse memory limit if provided
    let memory_limit_mb = if let Some(memory_str) = &args.memory {
        Some(gflow::utils::parse_memory_limit(memory_str)?)
    } else {
        None
    };

    // Handle dependencies (mutually exclusive via clap)
    let (depends_on_ids, dependency_mode) = if let Some(ref deps_all) = args.depends_on_all {
        let ids = parse_dependency_list(deps_all, client).await?;
        (ids, Some(gflow::core::job::DependencyMode::All))
    } else if let Some(ref deps_any) = args.depends_on_any {
        let ids = parse_dependency_list(deps_any, client).await?;
        (ids, Some(gflow::core::job::DependencyMode::Any))
    } else if let Some(ref dep) = args.depends_on {
        // Legacy single dependency
        let dep_id = resolve_dependency(Some(dep.clone()), client).await?;
        if let Some(id) = dep_id {
            (vec![id], Some(gflow::core::job::DependencyMode::All))
        } else {
            (vec![], None)
        }
    } else {
        (vec![], None)
    };

    builder = builder.depends_on_ids(depends_on_ids.clone());
    builder = builder.dependency_mode(dependency_mode);
    builder = builder.auto_cancel_on_dependency_failure(!args.no_auto_cancel);

    // For backward compatibility, also set depends_on if there's exactly one dependency
    if depends_on_ids.len() == 1 {
        builder = builder.depends_on(Some(depends_on_ids[0]));
    }

    if let Some(content) = stdin_content {
        // Stdin mode - save content to a temporary script file
        let script_args = parse_script_content_for_args(content)?;
        let temp_script = save_stdin_to_temp_file(content)?;

        builder = builder.script(temp_script);
        builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
        builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
        builder = builder.conda_env(args.conda_env.or(script_args.conda_env));

        // CLI time limit takes precedence over script time limit
        let final_time_limit = if time_limit.is_some() {
            time_limit
        } else if let Some(script_time_str) = &script_args.time {
            Some(gflow::utils::parse_time_limit(script_time_str)?)
        } else {
            None
        };
        builder = builder.time_limit(final_time_limit);

        // CLI memory limit takes precedence over script memory limit
        let final_memory_limit = if memory_limit_mb.is_some() {
            memory_limit_mb
        } else if let Some(script_memory_str) = &script_args.memory {
            Some(gflow::utils::parse_memory_limit(script_memory_str)?)
        } else {
            None
        };
        builder = builder.memory_limit_mb(final_memory_limit);
    } else {
        // Determine if it's a script or command
        let is_script =
            args.script_or_command.len() == 1 && PathBuf::from(&args.script_or_command[0]).exists();

        if is_script {
            // Script mode
            let script_path = make_absolute_path(PathBuf::from(&args.script_or_command[0]))?;
            let script_args = parse_script_for_args(&script_path)?;

            builder = builder.script(script_path);
            builder = builder.gpus(args.gpus.or(script_args.gpus).unwrap_or(0));
            builder = builder.priority(args.priority.or(script_args.priority).unwrap_or(10));
            builder = builder.conda_env(args.conda_env.or(script_args.conda_env));

            // CLI time limit takes precedence over script time limit
            let final_time_limit = if time_limit.is_some() {
                time_limit
            } else if let Some(script_time_str) = &script_args.time {
                Some(gflow::utils::parse_time_limit(script_time_str)?)
            } else {
                None
            };
            builder = builder.time_limit(final_time_limit);

            // CLI memory limit takes precedence over script memory limit
            let final_memory_limit = if memory_limit_mb.is_some() {
                memory_limit_mb
            } else if let Some(script_memory_str) = &script_args.memory {
                Some(gflow::utils::parse_memory_limit(script_memory_str)?)
            } else {
                None
            };
            builder = builder.memory_limit_mb(final_memory_limit);
        } else {
            // Command mode
            let command = args
                .script_or_command
                .iter()
                .map(|arg| shell_escape::escape(arg.into()))
                .collect::<Vec<_>>()
                .join(" ");
            builder = builder.command(command);
            builder = builder.gpus(args.gpus.unwrap_or(0));
            builder = builder.priority(args.priority.unwrap_or(10));

            // Auto-detect conda environment if not specified
            let conda_env = args.conda_env.or_else(detect_current_conda_env);
            builder = builder.conda_env(conda_env);

            builder = builder.time_limit(time_limit);
            builder = builder.memory_limit_mb(memory_limit_mb);
        }
    }

    // Set auto-close tmux flag
    builder = builder.auto_close_tmux(args.auto_close);

    Ok(builder.build())
}

fn parse_array_spec(spec: &str) -> Result<Vec<u32>> {
    if let Some(parts) = spec.split_once('-') {
        let start = parts
            .0
            .parse::<u32>()
            .context("Invalid array start index")?;
        let end = parts.1.parse::<u32>().context("Invalid array end index")?;
        if start > end {
            return Err(anyhow!(
                "Array start index cannot be greater than end index"
            ));
        }
        Ok((start..=end).collect())
    } else {
        Err(anyhow!(
            "Invalid array format. Expected format like '1-10'."
        ))
    }
}

fn parse_script_for_args(script_path: &PathBuf) -> Result<cli::AddArgs> {
    let content = fs::read_to_string(script_path).context("Failed to read script file")?;
    parse_script_content_for_args(&content)
}

fn parse_script_content_for_args(content: &str) -> Result<cli::AddArgs> {
    let gflow_lines: Vec<&str> = content
        .lines()
        .filter(|line| line.starts_with("# GFLOW"))
        .map(|line| line.trim_start_matches("# GFLOW").trim())
        .collect();

    if gflow_lines.is_empty() {
        return Ok(cli::AddArgs {
            script_or_command: vec![],
            conda_env: None,
            gpus: None,
            priority: None,
            depends_on: None,
            depends_on_all: None,
            depends_on_any: None,
            no_auto_cancel: false,
            array: None,
            time: None,
            memory: None,
            name: None,
            auto_close: false,
            param: vec![],
            dry_run: false,
            max_concurrent: None,
            param_file: None,
            name_template: None,
        });
    }

    let args_str = gflow_lines.join(" ");
    // Add a dummy positional arg since we only care about the options
    let full_args = format!("gbatch {args_str} dummy");
    let parsed = cli::GBatch::try_parse_from(full_args.split_whitespace())?;
    Ok(parsed.add_args)
}

fn save_stdin_to_temp_file(content: &str) -> Result<PathBuf> {
    use std::io::Write;

    // Create a temporary file in the system temp directory
    let temp_dir = std::env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros();
    let temp_path = temp_dir.join(format!("gflow_stdin_{}.sh", timestamp));

    let mut file =
        fs::File::create(&temp_path).context("Failed to create temporary script file")?;
    file.write_all(content.as_bytes())
        .context("Failed to write to temporary script file")?;

    // Make the file executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_path, perms)?;
    }

    Ok(temp_path)
}

fn make_absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|pwd| pwd.join(path))
            .context("Failed to get current directory")
    }
}

async fn resolve_dependency(depends_on: Option<String>, client: &Client) -> Result<Option<u32>> {
    match depends_on {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(anyhow!("Dependency value cannot be empty"));
            }

            // Check if it's a shorthand expression
            if trimmed.starts_with('@') {
                let username = gflow::core::get_current_username();
                let resolved_id = client
                    .resolve_dependency(&username, trimmed)
                    .await
                    .with_context(|| format!("Failed to resolve dependency '{}'", trimmed))?;
                Ok(Some(resolved_id))
            } else {
                // Parse as numeric job ID
                let parsed = trimmed
                    .parse::<u32>()
                    .map_err(|_| anyhow!("Invalid dependency value: {trimmed}"))?;
                Ok(Some(parsed))
            }
        }
    }
}

/// Parse comma-separated dependency list with @ syntax support
/// Examples: "123,456,@", "@,@~1,789"
async fn parse_dependency_list(deps_str: &str, client: &Client) -> Result<Vec<u32>> {
    let mut resolved_deps = Vec::new();
    let username = gflow::core::get_current_username();

    for dep in deps_str.split(',') {
        let trimmed = dep.trim();
        if trimmed.is_empty() {
            continue;
        }

        let dep_id = if trimmed.starts_with('@') {
            // Resolve shorthand
            client
                .resolve_dependency(&username, trimmed)
                .await
                .with_context(|| format!("Failed to resolve dependency '{}'", trimmed))?
        } else {
            // Parse as numeric ID
            trimmed
                .parse::<u32>()
                .with_context(|| format!("Invalid dependency ID: {}", trimmed))?
        };

        resolved_deps.push(dep_id);
    }

    if resolved_deps.is_empty() {
        anyhow::bail!("Dependency list cannot be empty");
    }

    Ok(resolved_deps)
}
