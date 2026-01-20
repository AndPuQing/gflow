use anyhow::Result;
use gflow::{client::Client, core::job::JobState, tmux::get_all_session_names};
use owo_colors::OwoColorize;
use std::collections::{HashMap, HashSet};
use tabled::{builder::Builder, settings::style::Style};

// Tree rendering constants - solid lines for dependencies
const TREE_BRANCH: &str = "├─";
const TREE_EDGE: &str = "╰─";
const TREE_PIPE: &str = "│ ";
const TREE_EMPTY: &str = "  ";

// Tree rendering constants - dashed lines for redo relationships
const TREE_BRANCH_DASHED: &str = "├┄";
const TREE_EDGE_DASHED: &str = "╰┄";

pub struct ListOptions {
    pub states: Option<String>,
    pub jobs: Option<String>,
    pub names: Option<String>,
    pub sort: String,
    pub limit: i32,
    pub all: bool,
    pub completed: bool,
    pub since: Option<String>,
    pub group: bool,
    pub tree: bool,
    pub format: Option<String>,
}

pub async fn handle_list(client: &Client, options: ListOptions) -> Result<()> {
    let current_user = gflow::core::get_current_username();

    // Determine states to query
    let states_filter = if options.completed {
        // Only completed states
        Some(
            JobState::completed_states()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    } else {
        // Use explicit --states if provided
        options.states.clone()
    };

    // Parse --since time filter if provided
    let created_after = if let Some(ref since_str) = options.since {
        Some(gflow::utils::parse_since_time(since_str)?)
    } else {
        None
    };

    // Query jobs with filters
    // Note: All jobs are stored in-memory on the server, not in a database
    let mut jobs_vec = client
        .list_jobs_with_query(
            states_filter,
            Some(current_user.clone()),
            None,
            None,
            created_after,
        )
        .await?;

    if let Some(job_ids) = options.jobs {
        let job_ids_vec: Vec<u32> = job_ids
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !job_ids_vec.is_empty() {
            jobs_vec.retain(|job| job_ids_vec.contains(&job.id));
        }
    }

    if let Some(names_filter) = options.names {
        let names_vec: Vec<String> = names_filter
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        if !names_vec.is_empty() {
            jobs_vec.retain(|job| {
                job.run_name
                    .as_ref()
                    .is_some_and(|run_name| names_vec.contains(run_name))
            });
        }
    }

    if jobs_vec.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    // Sort jobs
    sort_jobs(&mut jobs_vec, &options.sort);

    // Apply limit
    let effective_limit = if options.all { 0 } else { options.limit };
    if effective_limit != 0 {
        let total_jobs = jobs_vec.len();

        if effective_limit > 0 {
            // Positive limit: show first N jobs
            let limit_usize = effective_limit as usize;
            if jobs_vec.len() > limit_usize {
                jobs_vec.truncate(limit_usize);
                println!(
                    "Showing first {} of {} jobs (use --all or -n 0 to show all)",
                    effective_limit, total_jobs
                );
                println!();
            }
        } else {
            // Negative limit: show last N jobs
            let limit_usize = (-effective_limit) as usize;
            if jobs_vec.len() > limit_usize {
                let start = jobs_vec.len() - limit_usize;
                jobs_vec = jobs_vec.into_iter().skip(start).collect();
                println!(
                    "Showing last {} of {} jobs (use --all or -n 0 to show all)",
                    limit_usize, total_jobs
                );
                println!();
            }
        }
    }

    // Get all tmux sessions once upfront for efficiency
    let tmux_sessions = get_all_session_names();

    // Group by state if requested
    if options.group {
        display_grouped_jobs(jobs_vec, options.format.as_deref(), &tmux_sessions);
    } else if options.tree {
        display_jobs_tree(jobs_vec, options.format.as_deref(), &tmux_sessions);
    } else {
        display_jobs_table(jobs_vec, options.format.as_deref(), &tmux_sessions);
    }

    Ok(())
}

/// Sorts jobs by the specified field
fn sort_jobs(jobs: &mut [gflow::core::job::Job], sort_field: &str) {
    match sort_field.to_lowercase().as_str() {
        "id" => jobs.sort_by_key(|j| j.id),
        "state" => jobs.sort_by_key(|j| j.state),
        "time" => jobs.sort_by(|a, b| a.started_at.cmp(&b.started_at)),
        "name" => jobs.sort_by(|a, b| {
            a.run_name
                .as_deref()
                .unwrap_or("")
                .cmp(b.run_name.as_deref().unwrap_or(""))
        }),
        "gpus" | "nodes" => jobs.sort_by_key(|j| j.gpus),
        "priority" => jobs.sort_by_key(|j| j.priority),
        _ => {
            eprintln!(
                "Warning: Unknown sort field '{}', using default 'id'",
                sort_field
            );
            jobs.sort_by_key(|j| j.id);
        }
    }
}

/// Displays jobs in a standard table format
fn display_jobs_table(
    jobs: Vec<gflow::core::job::Job>,
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let headers: Vec<&str> = format.split(',').collect();

    // Build table using tabled Builder
    let mut builder = Builder::default();

    // Add header row
    builder.push_record(headers.clone());

    // Add data rows
    for job in jobs {
        let row: Vec<String> = headers
            .iter()
            .map(|header| format_job_cell(&job, header, tmux_sessions))
            .collect();
        builder.push_record(row);
    }

    let mut table = builder.build();
    table.with(Style::blank());

    println!("{}", table);
}

fn display_grouped_jobs(
    jobs: Vec<gflow::core::job::Job>,
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
) {
    use gflow::core::job::JobState;

    let mut grouped = std::collections::HashMap::new();
    for job in jobs {
        grouped.entry(job.state).or_insert_with(Vec::new).push(job);
    }

    let states_order = [
        JobState::Running,
        JobState::Queued,
        JobState::Finished,
        JobState::Failed,
        JobState::Cancelled,
        JobState::Timeout,
    ];

    let mut first = true;
    for state in states_order {
        if let Some(state_jobs) = grouped.get(&state) {
            if !first {
                println!();
            }
            first = false;

            println!("{} ({})", state, state_jobs.len());
            println!("{}", "─".repeat(60));
            display_jobs_table(state_jobs.clone(), format, tmux_sessions);
        }
    }
}

/// Colorizes a job state string based on its state
fn colorize_state(state: &JobState) -> String {
    let short = state.short_form();
    match state {
        JobState::Running => short.green().bold().to_string(),
        JobState::Finished => short.dimmed().to_string(),
        JobState::Queued => short.italic().to_string(),
        JobState::Hold => short.bold().to_string(),
        JobState::Failed => short.red().bold().to_string(),
        JobState::Timeout => short.underline().to_string(),
        JobState::Cancelled => short.strikethrough().to_string(),
    }
}

/// Computes the reason why a job is in its current state for display
fn get_job_reason_display(job: &gflow::core::job::Job) -> String {
    use gflow::core::job::JobStateReason;

    // If job already has a reason set, use it (except for CancelledByUser)
    if let Some(reason) = &job.reason {
        if matches!(reason, JobStateReason::CancelledByUser) {
            return "-".to_string();
        }
        return format!("({})", reason);
    }

    // Compute the reason based on state
    match job.state {
        JobState::Hold => format!("({})", JobStateReason::JobHeldUser),
        JobState::Queued => {
            let has_dependencies = job.depends_on.is_some() || !job.depends_on_ids.is_empty();
            if has_dependencies {
                format!("({})", JobStateReason::WaitingForDependency)
            } else {
                format!("({})", JobStateReason::WaitingForResources)
            }
        }
        JobState::Cancelled => "-".to_string(),
        _ => "-".to_string(),
    }
}

/// Formats GPU IDs as a comma-separated string
fn format_gpu_ids(gpu_ids: Option<&Vec<u32>>) -> String {
    gpu_ids.map_or_else(
        || "-".to_string(),
        |ids| {
            ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        },
    )
}

/// Formats a job field value for display
fn format_job_cell(
    job: &gflow::core::job::Job,
    header: &str,
    tmux_sessions: &HashSet<String>,
) -> String {
    match header {
        "JOBID" => job.id.to_string(),
        "NAME" => format_job_name_with_session_status(job, tmux_sessions),
        "ST" => colorize_state(&job.state),
        "NODES" => job.gpus.to_string(),
        "MEMORY" => job
            .memory_limit_mb
            .map_or_else(|| "-".to_string(), gflow::utils::format_memory),
        "NODELIST(REASON)" => {
            // For running jobs, show GPU IDs
            // For queued/held/cancelled jobs, show pending reason
            match job.state {
                JobState::Running => format_gpu_ids(job.gpu_ids.as_ref()),
                JobState::Queued | JobState::Hold | JobState::Cancelled => {
                    get_job_reason_display(job)
                }
                _ => "-".to_string(),
            }
        }
        "TIME" => gflow::utils::format_elapsed_time(job.started_at, job.finished_at),
        "TIMELIMIT" => job
            .time_limit
            .map_or_else(|| "UNLIMITED".to_string(), gflow::utils::format_duration),
        "USER" => job.submitted_by.clone(),
        _ => String::new(),
    }
}

/// Formats the job name with a visual indicator for tmux session status
fn format_job_name_with_session_status(
    job: &gflow::core::job::Job,
    tmux_sessions: &HashSet<String>,
) -> String {
    let Some(name) = &job.run_name else {
        return "-".to_string();
    };

    if tmux_sessions.contains(name) {
        format!("{} {}", name, "○".green())
    } else {
        name.to_string()
    }
}

/// Tree structure for dependency visualization
struct JobNode {
    job: gflow::core::job::Job,
    children: Vec<JobNodeChild>,
}

/// Represents a child in the tree - either a real job node or a reference
enum JobNodeChild {
    Node(Box<JobNode>, bool), // (node, is_redo_relationship)
    Reference(u32),           // Reference to a job ID that appears elsewhere
}

/// Context for rendering jobs with formatting and session information
struct RenderContext<'a> {
    headers: &'a [&'a str],
    tmux_sessions: &'a HashSet<String>,
}

/// Builds a dependency tree from a list of jobs, with cycle detection
fn build_dependency_tree(jobs: Vec<gflow::core::job::Job>) -> Vec<JobNode> {
    // Create a map of job_id -> job for quick lookup
    let job_map: HashMap<u32, gflow::core::job::Job> =
        jobs.iter().map(|j| (j.id, j.clone())).collect();

    // Create a map of parent_id -> child jobs (for dependency relationships)
    let mut children_map: HashMap<Option<u32>, Vec<u32>> = HashMap::new();

    // Create a map of original_job_id -> redo jobs (for redo relationships)
    let mut redo_map: HashMap<u32, Vec<u32>> = HashMap::new();

    // Track all jobs that appear as dependency children (globally)
    let mut all_dependency_children: HashSet<u32> = HashSet::new();

    for job in &jobs {
        children_map.entry(job.depends_on).or_default().push(job.id);

        // Track all jobs that have a dependency parent
        if job.depends_on.is_some() {
            all_dependency_children.insert(job.id);
        }

        if let Some(redone_from) = job.redone_from {
            redo_map.entry(redone_from).or_default().push(job.id);
        }
    }

    // Build tree nodes recursively with cycle detection
    fn build_node(
        job_id: u32,
        job_map: &HashMap<u32, gflow::core::job::Job>,
        children_map: &HashMap<Option<u32>, Vec<u32>>,
        redo_map: &HashMap<u32, Vec<u32>>,
        all_dependency_children: &HashSet<u32>,
        visited: &mut HashSet<u32>,
        recursion_stack: &mut HashSet<u32>,
    ) -> Option<JobNode> {
        // Check for circular dependency
        if recursion_stack.contains(&job_id) {
            tracing::warn!(
                "Circular dependency detected for job {}, skipping subtree",
                job_id
            );
            return None;
        }

        // Check if job exists in the map
        let job = job_map.get(&job_id)?.clone();

        // Mark as visited and in recursion stack
        visited.insert(job_id);
        recursion_stack.insert(job_id);

        // Collect dependency children IDs first
        let dep_child_ids: HashSet<u32> = children_map
            .get(&Some(job_id))
            .into_iter()
            .flatten()
            .copied()
            .collect();

        let dep_iter = dep_child_ids.iter().map(|&id| (id, false));

        // For redo children that are dependency children elsewhere, create references
        let redo_iter = redo_map
            .get(&job_id)
            .into_iter()
            .flatten()
            .map(|&id| (id, true));

        let mut children: Vec<JobNodeChild> = dep_iter
            .chain(redo_iter)
            .filter_map(|(child_id, is_redo)| {
                // If this is a redo child that appears as a dependency child elsewhere,
                // create a reference instead of a full node
                if is_redo && all_dependency_children.contains(&child_id) {
                    Some(JobNodeChild::Reference(child_id))
                } else {
                    build_node(
                        child_id,
                        job_map,
                        children_map,
                        redo_map,
                        all_dependency_children,
                        visited,
                        recursion_stack,
                    )
                    .map(|child_node| JobNodeChild::Node(Box::new(child_node), is_redo))
                }
            })
            .collect();

        // Sort children by job ID to maintain proper ordering
        children.sort_by_key(|child| match child {
            JobNodeChild::Node(node, _) => node.job.id,
            JobNodeChild::Reference(id) => *id,
        });

        // Remove from recursion stack (backtrack)
        recursion_stack.remove(&job_id);

        Some(JobNode { job, children })
    }

    // Find root jobs (jobs with no dependencies or dependencies not in the list)
    let mut root_ids = children_map.get(&None).cloned().unwrap_or_default();

    // Exclude jobs that have redone_from relationships where the original job exists in the list
    // These jobs will be displayed as children of their original jobs with dashed lines
    root_ids.retain(|job_id| {
        let parent_exists = job_map
            .get(job_id)
            .and_then(|job| job.redone_from)
            .is_some_and(|parent_id| job_map.contains_key(&parent_id));

        !parent_exists
    });

    let mut visited = HashSet::new();
    let mut recursion_stack = HashSet::new();

    root_ids
        .into_iter()
        .filter_map(|job_id| {
            build_node(
                job_id,
                &job_map,
                &children_map,
                &redo_map,
                &all_dependency_children,
                &mut visited,
                &mut recursion_stack,
            )
        })
        .collect()
}

/// Displays jobs in a tree format showing dependency relationships
fn display_jobs_tree(
    jobs: Vec<gflow::core::job::Job>,
    format: Option<&str>,
    tmux_sessions: &HashSet<String>,
) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let headers: Vec<&str> = format.split(',').collect();

    // Build dependency tree
    let tree = build_dependency_tree(jobs.clone());

    // Build table using tabled Builder
    let mut builder = Builder::default();

    // Add header row
    builder.push_record(headers.clone());

    // Create render context
    let ctx = RenderContext {
        headers: &headers,
        tmux_sessions,
    };

    // Collect all tree rows
    for node in &tree {
        collect_tree_rows(&mut builder, node, &ctx, "", true, true, false);
    }

    let mut table = builder.build();
    table.with(Style::blank());

    println!("{}", table);
}

/// Collects job node and its children as table rows
fn collect_tree_rows(
    builder: &mut Builder,
    node: &JobNode,
    ctx: &RenderContext,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    is_redo: bool,
) {
    let job = &node.job;
    let tree_prefix = if is_root {
        String::new()
    } else if is_redo {
        // Use dashed lines for redo relationships
        if is_last {
            TREE_EDGE_DASHED.to_string()
        } else {
            TREE_BRANCH_DASHED.to_string()
        }
    } else {
        // Use solid lines for dependency relationships
        if is_last {
            TREE_EDGE.to_string()
        } else {
            TREE_BRANCH.to_string()
        }
    };

    // Build the row
    let row: Vec<String> = ctx
        .headers
        .iter()
        .enumerate()
        .map(|(idx, header)| {
            if *header == "JOBID" && idx == 0 {
                // Add tree prefix to JOBID column
                format!("{}{}{}", prefix, tree_prefix, job.id)
            } else {
                format_job_cell(job, header, ctx.tmux_sessions)
            }
        })
        .collect();

    builder.push_record(row);

    // Collect children with updated prefix
    let child_count = node.children.len();
    for (idx, child) in node.children.iter().enumerate() {
        let is_last_child = idx == child_count - 1;

        // Root nodes should not add any prefix to their children
        // Non-root nodes add TREE_PIPE if not last, TREE_EMPTY if last (to maintain tree structure)
        let child_prefix = if is_root {
            String::new()
        } else {
            // Use solid pipe for dependency relationships
            if is_last {
                format!("{}{}", prefix, TREE_EMPTY)
            } else {
                format!("{}{}", prefix, TREE_PIPE)
            }
        };

        match child {
            JobNodeChild::Node(child_node, child_is_redo) => {
                collect_tree_rows(
                    builder,
                    child_node,
                    ctx,
                    &child_prefix,
                    is_last_child,
                    false,
                    *child_is_redo,
                );
            }
            JobNodeChild::Reference(job_id) => {
                // Add a reference row - make it compact by using minimal spacing
                let tree_prefix = if is_last_child {
                    TREE_EDGE_DASHED
                } else {
                    TREE_BRANCH_DASHED
                };

                // Create a compact reference that doesn't cause large gaps
                let reference_text = format!("{}{}→ see job {}", child_prefix, tree_prefix, job_id);

                let row: Vec<String> = ctx
                    .headers
                    .iter()
                    .enumerate()
                    .map(|(idx, header)| {
                        if *header == "JOBID" && idx == 0 {
                            reference_text.clone()
                        } else {
                            // Use "-" for other columns to maintain table structure
                            "-".to_string()
                        }
                    })
                    .collect();

                builder.push_record(row);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::{Job, JobState};
    use std::path::PathBuf;

    fn create_test_job(id: u32, name: &str, depends_on: Option<u32>) -> Job {
        Job {
            id,
            script: None,
            command: Some(format!("test command {}", id)),
            gpus: 1,
            conda_env: None,
            run_dir: PathBuf::from("/tmp"),
            priority: 10,
            depends_on,
            depends_on_ids: vec![],
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: None,
            run_name: Some(name.to_string()),
            state: JobState::Finished,
            gpu_ids: Some(vec![0]),
            started_at: None,
            finished_at: None,
            time_limit: None,
            memory_limit_mb: None,
            submitted_by: "testuser".to_string(),
            redone_from: None,
            auto_close_tmux: false,
            parameters: std::collections::HashMap::new(),
            group_id: None,
            max_concurrent: None,
            reason: None,
        }
    }

    fn create_test_job_with_state(id: u32, name: &str, state: JobState) -> Job {
        Job {
            id,
            script: None,
            command: Some(format!("test command {}", id)),
            gpus: 1,
            conda_env: None,
            run_dir: PathBuf::from("/tmp"),
            priority: 10,
            depends_on: None,
            depends_on_ids: vec![],
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: None,
            run_name: Some(name.to_string()),
            state,
            gpu_ids: Some(vec![0]),
            started_at: None,
            finished_at: None,
            time_limit: None,
            memory_limit_mb: None,
            submitted_by: "testuser".to_string(),
            redone_from: None,
            auto_close_tmux: false,
            parameters: std::collections::HashMap::new(),
            group_id: None,
            max_concurrent: None,
            reason: None,
        }
    }

    fn create_test_job_with_redo(id: u32, name: &str, redone_from: Option<u32>) -> Job {
        Job {
            id,
            script: None,
            command: Some(format!("test command {}", id)),
            gpus: 1,
            conda_env: None,
            run_dir: PathBuf::from("/tmp"),
            priority: 10,
            depends_on: None,
            depends_on_ids: vec![],
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: None,
            run_name: Some(name.to_string()),
            state: JobState::Finished,
            gpu_ids: Some(vec![0]),
            started_at: None,
            finished_at: None,
            time_limit: None,
            memory_limit_mb: None,
            submitted_by: "testuser".to_string(),
            redone_from,
            auto_close_tmux: false,
            parameters: std::collections::HashMap::new(),
            group_id: None,
            max_concurrent: None,
            reason: None,
        }
    }

    #[test]
    fn test_statue() {
        let jobs = vec![
            create_test_job_with_state(1, "job-1", JobState::Running),
            create_test_job_with_state(2, "job-2", JobState::Finished),
            create_test_job_with_state(3, "job-3", JobState::Queued),
            create_test_job_with_state(4, "job-4", JobState::Hold),
            create_test_job_with_state(5, "job-5", JobState::Failed),
            create_test_job_with_state(6, "job-6", JobState::Timeout),
            create_test_job_with_state(7, "job-7", JobState::Cancelled),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_simple_dependency_tree() {
        let jobs = vec![
            create_test_job(1, "root-job", None),
            create_test_job(2, "child-job-1", Some(1)),
            create_test_job(3, "child-job-2", Some(1)),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_multi_level_dependency_tree() {
        let jobs = vec![
            create_test_job(1, "root-job", None),
            create_test_job(2, "level-1-job", Some(1)),
            create_test_job(3, "level-2-job", Some(2)),
            create_test_job(4, "level-3-job", Some(3)),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_multiple_independent_trees() {
        let jobs = vec![
            create_test_job(1, "root-1", None),
            create_test_job(2, "child-1-1", Some(1)),
            create_test_job(3, "root-2", None),
            create_test_job(4, "child-2-1", Some(3)),
            create_test_job(5, "child-2-2", Some(3)),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_circular_dependency_detection() {
        // Note: This creates a simulated circular dependency scenario
        // In reality, the job system should prevent this at submission time
        let jobs = vec![
            create_test_job(1, "job-1", None),
            create_test_job(2, "job-2", Some(1)),
            create_test_job(3, "job-3", Some(2)),
            // If job 1 depended on 3, it would be circular, but we can't represent
            // this in our current structure without modifying the data after creation
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_missing_parent_job() {
        let jobs = vec![
            create_test_job(1, "job-1", None),
            create_test_job(2, "job-2", Some(99)), // Parent 99 doesn't exist
            create_test_job(3, "job-3", Some(1)),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_gap_job() {
        let jobs = vec![
            create_test_job(1, "job-1", None),
            create_test_job(2, "job-2", None), // Parent 99 doesn't exist
            create_test_job(3, "job-3", Some(1)),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_complex_branching_tree() {
        let jobs = vec![
            create_test_job(1, "root", None),
            create_test_job(2, "branch-a", Some(1)),
            create_test_job(3, "branch-b", Some(1)),
            create_test_job(4, "branch-a-1", Some(2)),
            create_test_job(5, "branch-a-2", Some(2)),
            create_test_job(6, "branch-b-1", Some(3)),
            create_test_job(7, "deep-child", Some(4)),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_empty_job_list() {
        let jobs: Vec<Job> = vec![];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_tree_with_long_job_names() {
        let jobs = vec![
            create_test_job(1, "very-long-root-job-name-here", None),
            create_test_job(2, "extremely-long-child-job-name", Some(1)),
            create_test_job(3, "short", Some(1)),
        ];
        println!();
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_redo_relationship() {
        // Test showing redo relationships with dashed lines
        let jobs = vec![
            create_test_job(1, "original-job", None),
            create_test_job(2, "dependent-job", Some(1)),
            create_test_job_with_redo(3, "redo-of-job-1", Some(1)),
        ];
        println!();
        println!("Test: Redo relationship (job 3 is redone from job 1)");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_mixed_dependencies_and_redo() {
        // Test showing both dependency (solid) and redo (dashed) relationships
        let jobs = vec![
            create_test_job(1, "root", None),
            create_test_job(2, "child-dep", Some(1)), // Depends on 1 (solid line)
            create_test_job_with_redo(3, "redo-1", Some(1)), // Redone from 1 (dashed line)
            create_test_job(4, "grandchild", Some(2)), // Depends on 2 (solid line)
        ];
        println!();
        println!("Test: Mixed dependencies and redo relationships");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_mixed_dependencies_and_redo_2() {
        let jobs = vec![
            create_test_job(1, "root", None),
            create_test_job_with_redo(2, "redo-1", Some(1)), // Depends on 1 (solid line)
            create_test_job_with_redo(3, "redo-1", Some(1)), // Redone from 1 (dashed line)
            create_test_job(4, "grandchild", Some(2)),       // Depends on 2 (solid line)
        ];
        println!();
        println!("Test: Mixed dependencies and redo relationships");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_job_with_both_dependency_and_redo() {
        // This test case matches the user's scenario:
        // Job 165 has both depends_on=163 and redone_from=164
        // It should only appear once (as a dependency child) with a reference indicator
        let mut job_165 = create_test_job(165, "gflow-job-165", Some(163));
        job_165.redone_from = Some(164);

        let jobs = vec![
            create_test_job(162, "gflow-job-162", None),
            create_test_job(163, "gflow-job-163", Some(162)),
            create_test_job(164, "gflow-job-164", Some(163)),
            job_165,
        ];
        println!();
        println!("Test: Job with both dependency and redo relationship (user's scenario)");
        println!("Job 165 depends on 163 AND is a redo of 164");
        println!("Expected: Job 165 appears once under 163, with '→ see job 165 below' reference under 164");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_repeated_redo_operations() {
        // Test case: Multiple redo operations on the same job
        // Job 100 -> Job 101 (redo of 100) -> Job 102 (redo of 101) -> Job 103 (redo of 102)
        let jobs = vec![
            create_test_job(100, "original-job", None),
            create_test_job_with_redo(101, "redo-1", Some(100)),
            create_test_job_with_redo(102, "redo-2", Some(101)),
            create_test_job_with_redo(103, "redo-3", Some(102)),
        ];
        println!();
        println!("Test: Repeated redo operations (chain of redos)");
        println!("100 -> 101 (redo of 100) -> 102 (redo of 101) -> 103 (redo of 102)");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_multiple_redos_of_same_job() {
        // Test case: Multiple jobs are redos of the same original job
        // Job 200 has three redos: 201, 202, 203
        let jobs = vec![
            create_test_job(200, "original-job", None),
            create_test_job_with_redo(201, "redo-attempt-1", Some(200)),
            create_test_job_with_redo(202, "redo-attempt-2", Some(200)),
            create_test_job_with_redo(203, "redo-attempt-3", Some(200)),
        ];
        println!();
        println!("Test: Multiple redos of the same job");
        println!("Jobs 201, 202, 203 are all redos of job 200");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_redo_with_dependencies() {
        // Test case: A redo job that has its own dependencies
        // Job 300 -> Job 301 (depends on 300)
        // Job 302 (redo of 300) -> Job 303 (depends on 302)
        let jobs = vec![
            create_test_job(300, "original-job", None),
            create_test_job(301, "child-of-original", Some(300)),
            create_test_job_with_redo(302, "redo-job", Some(300)),
            create_test_job(303, "child-of-redo", Some(302)),
        ];
        println!();
        println!("Test: Redo job with its own dependencies");
        println!("300 -> 301 (depends on 300)");
        println!("302 (redo of 300) -> 303 (depends on 302)");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_complex_redo_and_dependency_mix() {
        // Test case: Complex scenario with both dependencies and redos
        // Job 400 -> Job 401 (depends on 400) -> Job 402 (depends on 401)
        // Job 403 (redo of 401, also depends on 400)
        // Job 404 (redo of 402, also depends on 403)
        let mut job_403 = create_test_job(403, "redo-of-401", Some(400));
        job_403.redone_from = Some(401);

        let mut job_404 = create_test_job(404, "redo-of-402", Some(403));
        job_404.redone_from = Some(402);

        let jobs = vec![
            create_test_job(400, "root-job", None),
            create_test_job(401, "child-1", Some(400)),
            create_test_job(402, "grandchild", Some(401)),
            job_403,
            job_404,
        ];
        println!();
        println!("Test: Complex mix of dependencies and redos");
        println!("400 -> 401 -> 402");
        println!("403 (redo of 401, depends on 400)");
        println!("404 (redo of 402, depends on 403)");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_redo_chain_with_dependency_conflict() {
        // Test case: A chain where a redo appears as both a redo child and dependency child
        // Job 500 -> Job 501 (depends on 500)
        // Job 502 (redo of 500, depends on 501) - should appear under 501, with reference under 500
        let mut job_502 = create_test_job(502, "redo-depends-on-child", Some(501));
        job_502.redone_from = Some(500);

        let jobs = vec![
            create_test_job(500, "original", None),
            create_test_job(501, "child", Some(500)),
            job_502,
        ];
        println!();
        println!("Test: Redo chain with dependency conflict");
        println!("500 -> 501");
        println!("502 (redo of 500, but depends on 501)");
        println!("Expected: 502 appears under 501, reference under 500");
        display_jobs_tree(jobs, None, &HashSet::new());
    }

    #[test]
    fn test_multiple_redo_references_same_job() {
        // Test case: Multiple jobs have redo relationships pointing to the same job
        // that appears as a dependency child
        // Job 600 -> Job 601 (depends on 600) -> Job 602 (depends on 601)
        // Job 603 (redo of 602, no dependency)
        // Job 604 (redo of 602, no dependency)
        // Expected: 602 appears under 601, both 603 and 604 show references
        let job_603 = create_test_job_with_redo(603, "redo-1-of-602", Some(602));
        let job_604 = create_test_job_with_redo(604, "redo-2-of-602", Some(602));

        let jobs = vec![
            create_test_job(600, "root", None),
            create_test_job(601, "child", Some(600)),
            create_test_job(602, "grandchild", Some(601)),
            job_603,
            job_604,
        ];
        println!();
        println!("Test: Multiple redo references to same job");
        println!("600 -> 601 -> 602");
        println!("603 and 604 are both redos of 602");
        println!("Expected: 602 appears under 601, 603 and 604 are root jobs with redo indicators");
        display_jobs_tree(jobs, None, &HashSet::new());
    }
}
