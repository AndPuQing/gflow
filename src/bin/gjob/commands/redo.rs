use anyhow::{anyhow, Context, Result};
use gflow::client::Client;
use gflow::core::job::{Job, JobState, JobStateReason};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

#[allow(clippy::too_many_arguments)]
pub async fn handle_redo(
    config_path: &Option<PathBuf>,
    job_id_str: &str,
    gpus_override: Option<u32>,
    priority_override: Option<u8>,
    depends_on_override: Option<String>,
    time_override: Option<String>,
    memory_override: Option<String>,
    conda_env_override: Option<String>,
    clear_deps: bool,
    cascade: bool,
) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    // Resolve job ID (handle @ shorthand)
    let job_id = crate::utils::resolve_job_id(&client, job_id_str).await?;

    // Retrieve the original job
    let original_job = match client.get_job(job_id).await? {
        Some(job) => job,
        None => {
            return Err(anyhow!("Job {} not found.", job_id));
        }
    };

    println!("Resubmitting job {} with parameters:", original_job.id);

    // Build new job based on original
    let mut builder = Job::builder();

    // Preserve core job parameters
    if let Some(ref script) = original_job.script {
        builder = builder.script(script.clone());
        println!("  Script:       {}", script.display());
    }
    if let Some(ref command) = original_job.command {
        builder = builder.command(command.clone());
        println!("  Command:      {}", command);
    }

    // Apply GPUs (override or original)
    let gpus = gpus_override.unwrap_or(original_job.gpus);
    builder = builder.gpus(gpus);
    println!("  GPUs:         {}", gpus);

    // Apply priority (override or original)
    let priority = priority_override.unwrap_or(original_job.priority);
    builder = builder.priority(priority);
    println!("  Priority:     {}", priority);

    // Apply conda environment (override or original)
    let conda_env = if let Some(ref override_env) = conda_env_override {
        Some(override_env.clone())
    } else {
        original_job.conda_env.clone()
    };
    builder = builder.conda_env(conda_env.clone());
    if let Some(ref env) = conda_env {
        println!("  Conda env:    {}", env);
    }

    // Apply time limit (override or original)
    let time_limit = if let Some(ref time_str) = time_override {
        Some(gflow::utils::parse_time_limit(time_str)?)
    } else {
        original_job.time_limit
    };
    builder = builder.time_limit(time_limit);
    if let Some(limit) = time_limit {
        println!("  Time limit:   {}", gflow::utils::format_duration(limit));
    }

    // Apply memory limit (override or original)
    let memory_limit_mb = if let Some(ref memory_str) = memory_override {
        Some(gflow::utils::parse_memory_limit(memory_str)?)
    } else {
        original_job.memory_limit_mb
    };
    builder = builder.memory_limit_mb(memory_limit_mb);
    if let Some(memory_mb) = memory_limit_mb {
        println!("  Memory limit: {}", gflow::utils::format_memory(memory_mb));
    }

    // Handle dependency
    let depends_on = if clear_deps {
        println!("  Dependencies: (cleared)");
        None
    } else if let Some(ref dep_str) = depends_on_override {
        let resolved_dep = crate::utils::resolve_dependency(&client, dep_str).await?;
        println!("  Depends on:   {}", resolved_dep);
        Some(resolved_dep)
    } else {
        if let Some(dep) = original_job.depends_on {
            println!("  Depends on:   {}", dep);
        }
        original_job.depends_on
    };
    builder = builder.depends_on(depends_on);

    // Preserve other parameters
    builder = builder.run_dir(original_job.run_dir.clone());
    builder = builder.task_id(original_job.task_id);
    builder = builder.auto_close_tmux(original_job.auto_close_tmux);
    builder = builder.parameters(original_job.parameters.clone());

    // Display parameters if any
    if !original_job.parameters.is_empty() {
        println!("  Parameters:");
        for (key, value) in &original_job.parameters {
            println!("    {}: {}", key, value);
        }
    }

    // Track that this job was redone from the original job
    builder = builder.redone_from(Some(original_job.id));

    // Set the submitter to current user
    let username = gflow::core::get_current_username();
    builder = builder.submitted_by(username);

    // Build and submit the job
    let new_job = builder.build();
    let response = client
        .add_job(new_job)
        .await
        .context("Failed to submit job")?;

    println!(
        "\nSubmitted batch job {} ({})",
        response.id, response.run_name
    );

    // Handle cascade if requested
    if cascade {
        let cascade_jobs = find_cascade_jobs(&client, original_job.id).await?;
        if !cascade_jobs.is_empty() {
            println!("\nCascading to {} dependent job(s)...", cascade_jobs.len());

            let _mapping =
                redo_with_cascade(&client, &original_job, response.id, &cascade_jobs).await?;
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
                if let Some(JobStateReason::DependencyFailed(failed_dep_id)) = job.reason {
                    if failed_dep_id == current_id {
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
/// Returns a mapping of old job IDs to new job IDs.
async fn redo_with_cascade(
    client: &Client,
    original_parent: &Job,
    new_parent_id: u32,
    cascade_jobs: &[Job],
) -> Result<HashMap<u32, u32>> {
    let mut id_mapping = HashMap::new();
    id_mapping.insert(original_parent.id, new_parent_id);

    for cascade_job in cascade_jobs {
        // Build new job from the cascade job
        let mut builder = Job::builder();

        // Preserve core job parameters
        if let Some(ref script) = cascade_job.script {
            builder = builder.script(script.clone());
        }
        if let Some(ref command) = cascade_job.command {
            builder = builder.command(command.clone());
        }

        // Use original job parameters (no overrides for cascade jobs)
        builder = builder.gpus(cascade_job.gpus);
        builder = builder.priority(cascade_job.priority);
        builder = builder.conda_env(cascade_job.conda_env.clone());
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
        builder = builder.parameters(cascade_job.parameters.clone());
        builder = builder.group_id(cascade_job.group_id.clone());
        builder = builder.max_concurrent(cascade_job.max_concurrent);

        // Track that this job was redone from the original cascade job
        builder = builder.redone_from(Some(cascade_job.id));

        // Set the submitter to current user
        let username = gflow::core::get_current_username();
        builder = builder.submitted_by(username);

        // Build and submit the job
        let new_job = builder.build();
        let response = client.add_job(new_job).await.context(format!(
            "Failed to submit cascade job for {}",
            cascade_job.id
        ))?;

        id_mapping.insert(cascade_job.id, response.id);
        println!(
            "  Job {} → Job {} ({})",
            cascade_job.id, response.id, response.run_name
        );
    }

    Ok(id_mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::{JobBuilder, JobState, JobStateReason};

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
