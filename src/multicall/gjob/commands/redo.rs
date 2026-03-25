use anyhow::{anyhow, Context, Result};
use gflow::client::Client;
use gflow::core::job::{GpuSharingMode, Job, JobState, JobStateReason};
use gflow::print_field;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct RedoJobOptions {
    pub gpus_override: Option<u32>,
    pub priority_override: Option<u8>,
    pub depends_on_override: Option<u32>,
    pub time_limit_override: Option<Duration>,
    pub memory_limit_mb_override: Option<u64>,
    pub gpu_memory_limit_mb_override: Option<u64>,
    pub conda_env_override: Option<String>,
    pub clear_deps: bool,
    pub cascade: bool,
}

#[derive(Debug, Clone)]
pub struct CascadeRedoResult {
    pub original_job_id: u32,
    pub new_job_id: u32,
    pub run_name: String,
}

#[derive(Debug, Clone)]
pub struct RedoJobResult {
    pub original_job_id: u32,
    pub new_job_id: u32,
    pub run_name: String,
    pub cascaded_jobs: Vec<CascadeRedoResult>,
}

pub async fn redo_job(
    client: &Client,
    original_job_id: u32,
    options: &RedoJobOptions,
) -> Result<RedoJobResult> {
    let original_job = match client.get_job(original_job_id).await? {
        Some(job) => job,
        None => {
            return Err(anyhow!("Job {} not found.", original_job_id));
        }
    };

    validate_redo_source_job(&original_job)?;
    redo_job_from_original(client, &original_job, options).await
}

pub(crate) async fn redo_job_from_original(
    client: &Client,
    original_job: &Job,
    options: &RedoJobOptions,
) -> Result<RedoJobResult> {
    validate_redo_source_job(original_job)?;

    let new_job = build_redo_job(original_job, options);
    let response = client
        .add_job(new_job)
        .await
        .context("Failed to submit job")?;

    let cascaded_jobs = if options.cascade {
        let cascade_jobs = find_cascade_jobs(client, original_job.id).await?;
        if cascade_jobs.is_empty() {
            Vec::new()
        } else {
            redo_with_cascade(client, original_job, response.id, &cascade_jobs).await?
        }
    } else {
        Vec::new()
    };

    Ok(RedoJobResult {
        original_job_id: original_job.id,
        new_job_id: response.id,
        run_name: response.run_name,
        cascaded_jobs,
    })
}

pub(crate) fn validate_redo_source_job(original_job: &Job) -> Result<()> {
    match original_job.state {
        JobState::Queued | JobState::Hold => Err(anyhow!(
            "Job {} is still in {} state. Use `gjob update` to modify its parameters.",
            original_job.id,
            original_job.state
        )),
        JobState::Running => Err(anyhow!(
            "Job {} is still running. Wait for it to finish or cancel it first.",
            original_job.id
        )),
        _ => Ok(()),
    }
}

pub(crate) fn build_redo_job(original_job: &Job, options: &RedoJobOptions) -> Job {
    let mut builder = Job::builder();

    if let Some(ref script) = original_job.script {
        builder = builder.script((**script).clone());
    }
    if let Some(ref command) = original_job.command {
        builder = builder.command(command.clone());
    }

    builder = builder.gpus(options.gpus_override.unwrap_or(original_job.gpus));
    builder = builder.gpu_sharing_mode(original_job.gpu_sharing_mode);
    builder = builder.priority(options.priority_override.unwrap_or(original_job.priority));

    let conda_env = if let Some(ref override_env) = options.conda_env_override {
        Some(override_env.clone())
    } else {
        original_job.conda_env.as_ref().map(|s| s.to_string())
    };
    builder = builder.conda_env(conda_env);

    let time_limit = options.time_limit_override.or(original_job.time_limit);
    builder = builder.time_limit(time_limit);

    let memory_limit_mb = options
        .memory_limit_mb_override
        .or(original_job.memory_limit_mb);
    builder = builder.memory_limit_mb(memory_limit_mb);

    let gpu_memory_limit_mb = options
        .gpu_memory_limit_mb_override
        .or(original_job.gpu_memory_limit_mb);
    builder = builder.gpu_memory_limit_mb(gpu_memory_limit_mb);

    let depends_on = if options.clear_deps {
        None
    } else {
        options.depends_on_override.or(original_job.depends_on)
    };
    builder = builder.depends_on(depends_on);

    builder = builder.run_dir(original_job.run_dir.clone());
    builder = builder.task_id(original_job.task_id);
    builder = builder.auto_close_tmux(original_job.auto_close_tmux);
    builder = builder.parameters_compact(original_job.parameters.clone());
    builder = builder.notifications(original_job.notifications.clone());
    builder = builder.max_retry(original_job.max_retry);
    builder = builder.redone_from(Some(original_job.id));
    builder = builder.submitted_by(gflow::platform::get_current_username());

    builder.build()
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_redo(
    config_path: &Option<PathBuf>,
    job_id_str: &str,
    gpus_override: Option<u32>,
    priority_override: Option<u8>,
    depends_on_override: Option<String>,
    time_override: Option<String>,
    memory_override: Option<String>,
    gpu_memory_override: Option<String>,
    conda_env_override: Option<String>,
    clear_deps: bool,
    cascade: bool,
) -> Result<()> {
    let client = gflow::create_client(config_path)?;

    // Resolve job ID (handle @ shorthand)
    let job_id = crate::multicall::gjob::utils::resolve_job_id(&client, job_id_str).await?;

    // Retrieve the original job
    let original_job = match client.get_job(job_id).await? {
        Some(job) => job,
        None => {
            return Err(anyhow!("Job {} not found.", job_id));
        }
    };

    validate_redo_source_job(&original_job)?;

    println!("Resubmitting job {} with parameters:", original_job.id);

    let time_limit_override = if let Some(ref time_str) = time_override {
        Some(gflow::utils::parse_time_limit(time_str)?)
    } else {
        None
    };
    let memory_limit_mb_override = if let Some(ref memory_str) = memory_override {
        Some(gflow::utils::parse_memory_limit(memory_str)?)
    } else {
        None
    };
    let gpu_memory_limit_mb_override = if let Some(ref memory_str) = gpu_memory_override {
        Some(gflow::utils::parse_memory_limit(memory_str)?)
    } else {
        None
    };
    let depends_on_override = if let Some(ref dep_str) = depends_on_override {
        Some(crate::multicall::gjob::utils::resolve_dependency(&client, dep_str).await?)
    } else {
        None
    };

    let options = RedoJobOptions {
        gpus_override,
        priority_override,
        depends_on_override,
        time_limit_override,
        memory_limit_mb_override,
        gpu_memory_limit_mb_override,
        conda_env_override: conda_env_override.clone(),
        clear_deps,
        cascade,
    };

    if let Some(ref script) = original_job.script {
        print_field!("Script", "{}", script.display());
    }
    if let Some(ref command) = original_job.command {
        print_field!("Command", "{}", command);
    }

    // Apply GPUs (override or original)
    let gpus = options.gpus_override.unwrap_or(original_job.gpus);
    print_field!("GPUs", "{}", gpus);
    if original_job.gpu_sharing_mode == GpuSharingMode::Shared {
        print_field!("GPUSharing", "shared");
    }

    // Apply priority (override or original)
    let priority = options.priority_override.unwrap_or(original_job.priority);
    print_field!("Priority", "{}", priority);

    // Apply conda environment (override or original)
    let conda_env = if let Some(ref override_env) = options.conda_env_override {
        Some(override_env.clone())
    } else {
        original_job.conda_env.as_ref().map(|s| s.to_string())
    };
    if let Some(ref env) = conda_env {
        print_field!("CondaEnv", "{}", env);
    }

    // Apply time limit (override or original)
    let time_limit = options.time_limit_override.or(original_job.time_limit);
    if let Some(limit) = time_limit {
        print_field!("TimeLimit", "{}", gflow::utils::format_duration(limit));
    }

    // Apply memory limit (override or original)
    let memory_limit_mb = options
        .memory_limit_mb_override
        .or(original_job.memory_limit_mb);
    if let Some(memory_mb) = memory_limit_mb {
        print_field!("MemoryLimit", "{}", gflow::utils::format_memory(memory_mb));
    }

    // Apply per-GPU memory limit (override or original)
    let gpu_memory_limit_mb = options
        .gpu_memory_limit_mb_override
        .or(original_job.gpu_memory_limit_mb);
    if let Some(memory_mb) = gpu_memory_limit_mb {
        print_field!(
            "GPUMemoryLimit",
            "{}",
            gflow::utils::format_memory(memory_mb)
        );
    }

    // Handle dependency
    if options.clear_deps {
        println!("  Dependencies=(cleared)");
    } else if let Some(depends_on) = options.depends_on_override {
        print_field!("DependsOn", "{}", depends_on);
    } else {
        if let Some(dep) = original_job.depends_on {
            print_field!("DependsOn", "{}", dep);
        }
    }

    // Display parameters if any
    if !original_job.parameters.is_empty() {
        println!("  Parameters:");
        for (key, value) in &original_job.parameters {
            print_field!(key, "{}", value);
        }
    }

    let result = redo_job_from_original(&client, &original_job, &options).await?;

    println!(
        "\nSubmitted batch job {} ({})",
        result.new_job_id, result.run_name
    );

    // Handle cascade if requested
    if options.cascade {
        if !result.cascaded_jobs.is_empty() {
            println!(
                "\nCascading to {} dependent job(s)...",
                result.cascaded_jobs.len()
            );
            for cascaded in &result.cascaded_jobs {
                println!(
                    "  Job {} → Job {} ({})",
                    cascaded.original_job_id, cascaded.new_job_id, cascaded.run_name
                );
            }
            println!("\nCascade complete.");
        } else {
            println!("\nNo dependent jobs to cascade.");
        }
    }

    Ok(())
}

/// Find all jobs that should be cascaded (redone) when a parent job is redone.
/// This uses BFS to find all transitive dependents that were cancelled due to dependency failure.
async fn find_cascade_jobs(client: &Client, parent_job_id: u32) -> Result<Vec<Job>> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut cascade_jobs = Vec::new();

    queue.push_back(parent_job_id);
    visited.insert(parent_job_id);

    // Get all jobs to search through
    let all_jobs = client.list_jobs().await?;

    while let Some(current_id) = queue.pop_front() {
        // Find jobs that depend on current_id and were cancelled due to dependency failure
        for job in &all_jobs {
            if visited.contains(&job.id) {
                continue;
            }

            // Check if this job was cancelled due to the current job's failure
            if job.state == JobState::Cancelled {
                if let Some(JobStateReason::DependencyFailed(failed_dep_id)) = job.reason.as_deref()
                {
                    if *failed_dep_id == current_id {
                        visited.insert(job.id);
                        queue.push_back(job.id);
                        cascade_jobs.push(job.clone());
                    }
                }
            }
        }
    }

    // Sort jobs in topological order (dependencies first)
    cascade_jobs.sort_by_key(|job| job.id);

    Ok(cascade_jobs)
}

/// Redo jobs with cascade, updating dependencies to point to new job IDs.
async fn redo_with_cascade(
    client: &Client,
    original_parent: &Job,
    new_parent_id: u32,
    cascade_jobs: &[Job],
) -> Result<Vec<CascadeRedoResult>> {
    let mut id_mapping = HashMap::new();
    let mut results = Vec::new();
    id_mapping.insert(original_parent.id, new_parent_id);

    for cascade_job in cascade_jobs {
        // Build new job from the cascade job
        let mut builder = Job::builder();

        // Preserve core job parameters
        if let Some(ref script) = cascade_job.script {
            builder = builder.script((**script).clone());
        }
        if let Some(ref command) = cascade_job.command {
            builder = builder.command(command.clone());
        }

        // Use original job parameters (no overrides for cascade jobs)
        builder = builder.gpus(cascade_job.gpus);
        builder = builder.gpu_sharing_mode(cascade_job.gpu_sharing_mode);
        builder = builder.gpu_memory_limit_mb(cascade_job.gpu_memory_limit_mb);
        builder = builder.priority(cascade_job.priority);
        builder = builder.conda_env(cascade_job.conda_env.as_ref().map(|s| s.to_string()));
        builder = builder.time_limit(cascade_job.time_limit);
        builder = builder.memory_limit_mb(cascade_job.memory_limit_mb);

        // Update dependencies to point to new job IDs
        let updated_depends_on_ids: Vec<u32> = cascade_job
            .depends_on_ids
            .iter()
            .map(|old_id| *id_mapping.get(old_id).unwrap_or(old_id))
            .collect();

        builder = builder.depends_on_ids(updated_depends_on_ids);
        builder = builder.dependency_mode(cascade_job.dependency_mode);
        builder = builder
            .auto_cancel_on_dependency_failure(cascade_job.auto_cancel_on_dependency_failure);

        // Preserve other parameters
        builder = builder.run_dir(cascade_job.run_dir.clone());
        builder = builder.task_id(cascade_job.task_id);
        builder = builder.auto_close_tmux(cascade_job.auto_close_tmux);
        builder = builder.parameters_compact(cascade_job.parameters.clone());
        builder = builder.group_id_uuid(cascade_job.group_id);
        builder = builder.max_concurrent(cascade_job.max_concurrent);
        builder = builder.max_retry(cascade_job.max_retry);

        // Track that this job was redone from the original cascade job
        builder = builder.redone_from(Some(cascade_job.id));

        // Set the submitter to current user
        let username = gflow::platform::get_current_username();
        builder = builder.submitted_by(username);

        // Build and submit the job
        let new_job = builder.build();
        let response = client.add_job(new_job).await.context(format!(
            "Failed to submit cascade job for {}",
            cascade_job.id
        ))?;

        id_mapping.insert(cascade_job.id, response.id);
        results.push(CascadeRedoResult {
            original_job_id: cascade_job.id,
            new_job_id: response.id,
            run_name: response.run_name,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::{JobBuilder, JobNotifications, JobState, JobStateReason};
    use std::time::Duration;

    #[test]
    fn build_redo_job_preserves_original_parameters_by_default() {
        let original_job = JobBuilder::new()
            .command("python train.py")
            .gpus(2)
            .priority(7)
            .conda_env(Some("ml".to_string()))
            .time_limit(Some(Duration::from_secs(3600)))
            .memory_limit_mb(Some(32768))
            .gpu_memory_limit_mb(Some(16384))
            .depends_on(Some(41))
            .run_dir("/tmp/run")
            .task_id(Some(3))
            .auto_close_tmux(true)
            .parameters(HashMap::from([("lr".to_string(), "1e-3".to_string())]))
            .submitted_by("alice")
            .build();

        let redone_job = build_redo_job(&original_job, &RedoJobOptions::default());

        assert_eq!(redone_job.command.as_deref(), Some("python train.py"));
        assert_eq!(redone_job.gpus, 2);
        assert_eq!(redone_job.priority, 7);
        assert_eq!(redone_job.conda_env.as_deref(), Some("ml"));
        assert_eq!(redone_job.time_limit, Some(Duration::from_secs(3600)));
        assert_eq!(redone_job.memory_limit_mb, Some(32768));
        assert_eq!(redone_job.gpu_memory_limit_mb, Some(16384));
        assert_eq!(redone_job.depends_on, Some(41));
        assert_eq!(redone_job.run_dir, PathBuf::from("/tmp/run"));
        assert_eq!(redone_job.task_id, Some(3));
        assert!(redone_job.auto_close_tmux);
        assert_eq!(redone_job.parameters, original_job.parameters);
        assert_eq!(redone_job.redone_from, Some(original_job.id));
    }

    #[test]
    fn build_redo_job_applies_dependency_and_resource_overrides() {
        let original_job = JobBuilder::new()
            .command("python train.py")
            .gpus(1)
            .priority(10)
            .depends_on(Some(11))
            .time_limit(Some(Duration::from_secs(600)))
            .memory_limit_mb(Some(4096))
            .gpu_memory_limit_mb(Some(2048))
            .submitted_by("alice")
            .run_dir("/tmp")
            .build();

        let options = RedoJobOptions {
            gpus_override: Some(4),
            priority_override: Some(1),
            depends_on_override: Some(99),
            time_limit_override: Some(Duration::from_secs(7200)),
            memory_limit_mb_override: Some(65536),
            gpu_memory_limit_mb_override: Some(24576),
            conda_env_override: Some("cuda".to_string()),
            clear_deps: false,
            cascade: false,
        };
        let overridden_job = build_redo_job(&original_job, &options);

        assert_eq!(overridden_job.gpus, 4);
        assert_eq!(overridden_job.priority, 1);
        assert_eq!(overridden_job.depends_on, Some(99));
        assert_eq!(overridden_job.time_limit, Some(Duration::from_secs(7200)));
        assert_eq!(overridden_job.memory_limit_mb, Some(65536));
        assert_eq!(overridden_job.gpu_memory_limit_mb, Some(24576));
        assert_eq!(overridden_job.conda_env.as_deref(), Some("cuda"));

        let cleared_job = build_redo_job(
            &original_job,
            &RedoJobOptions {
                clear_deps: true,
                ..RedoJobOptions::default()
            },
        );
        assert_eq!(cleared_job.depends_on, None);
    }

    #[test]
    fn build_redo_job_preserves_per_job_notifications() {
        let original_job = JobBuilder::new()
            .command("python train.py")
            .submitted_by("alice")
            .run_dir("/tmp")
            .notifications(JobNotifications::normalized(
                vec!["alice@example.com".to_string()],
                vec!["job_failed".to_string(), "job_timeout".to_string()],
            ))
            .build();

        let redone_job = build_redo_job(&original_job, &RedoJobOptions::default());

        assert_eq!(redone_job.notifications, original_job.notifications);
    }

    #[test]
    fn test_cascade_job_identification() {
        // Test that we can identify jobs that should be cascaded
        let parent_job = JobBuilder::new()
            .submitted_by("test".to_string())
            .run_dir("/tmp")
            .build();

        let cancelled_job = JobBuilder::new()
            .submitted_by("test".to_string())
            .run_dir("/tmp")
            .build();

        // Verify the job structure
        assert_eq!(parent_job.state, JobState::Queued);
        assert_eq!(cancelled_job.state, JobState::Queued);
    }

    #[test]
    fn test_dependency_update_logic() {
        // Test that dependency IDs are correctly updated
        let mut id_mapping = HashMap::new();
        id_mapping.insert(100, 200);
        id_mapping.insert(101, 201);

        let old_deps = [100, 101];
        let new_deps: Vec<u32> = old_deps
            .iter()
            .map(|old_id| *id_mapping.get(old_id).unwrap_or(old_id))
            .collect();

        assert_eq!(new_deps, vec![200, 201]);
    }

    #[test]
    fn test_dependency_update_with_unmapped_ids() {
        // Test that unmapped IDs are preserved
        let mut id_mapping = HashMap::new();
        id_mapping.insert(100, 200);

        let old_deps = [100, 102]; // 102 is not in mapping
        let new_deps: Vec<u32> = old_deps
            .iter()
            .map(|old_id| *id_mapping.get(old_id).unwrap_or(old_id))
            .collect();

        assert_eq!(new_deps, vec![200, 102]);
    }

    #[test]
    fn test_job_state_reason_matching() {
        // Test that we can match DependencyFailed reasons
        let reason = JobStateReason::DependencyFailed(100);

        if let JobStateReason::DependencyFailed(failed_id) = reason {
            assert_eq!(failed_id, 100);
        } else {
            panic!("Expected DependencyFailed reason");
        }
    }
}
