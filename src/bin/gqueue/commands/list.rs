use anyhow::Result;
use gflow::{client::Client, core::job::JobState};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

// Tree rendering constants
const TREE_BRANCH: &str = "├─";
const TREE_EDGE: &str = "└─";
const TREE_PIPE: &str = "│ ";
const TREE_EMPTY: &str = "  ";
const TREE_CHARS_PER_LEVEL: usize = 3; // "├─ " or "│  "

pub struct ListOptions {
    pub states: Option<String>,
    pub jobs: Option<String>,
    pub names: Option<String>,
    pub sort: String,
    pub limit: i32,
    pub all: bool,
    pub group: bool,
    pub tree: bool,
    pub format: Option<String>,
}

pub async fn handle_list(client: &Client, options: ListOptions) -> Result<()> {
    let mut jobs_vec = client.list_jobs().await?;

    // Apply filters
    if let Some(states_filter) = options.states {
        let states_vec: Vec<JobState> = states_filter
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !states_vec.is_empty() {
            jobs_vec.retain(|job| states_vec.contains(&job.state));
        }
    }

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

    // Group by state if requested
    if options.group {
        display_grouped_jobs(jobs_vec, options.format.as_deref());
    } else if options.tree {
        display_jobs_tree(jobs_vec, options.format.as_deref());
    } else {
        display_jobs_table(jobs_vec, options.format.as_deref());
    }

    Ok(())
}

/// Sorts jobs by the specified field
fn sort_jobs(jobs: &mut [gflow::core::job::Job], sort_field: &str) {
    match sort_field.to_lowercase().as_str() {
        "id" => jobs.sort_by_key(|j| j.id),
        "state" => jobs.sort_by_key(|j| j.state.clone()),
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
fn display_jobs_table(jobs: Vec<gflow::core::job::Job>, format: Option<&str>) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let headers: Vec<&str> = format.split(',').collect();

    // Calculate dynamic column widths based on actual content
    let widths: HashMap<&str, usize> = headers
        .iter()
        .map(|h| (*h, get_dynamic_width(h, &jobs)))
        .collect();

    // Print header with dynamic widths
    println!(
        "{}",
        headers
            .iter()
            .map(|h| format!("{:<width$}", h, width = widths[h]))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Print each job row with dynamic widths
    for job in jobs {
        let row: Vec<String> = headers
            .iter()
            .map(|header| {
                let value = format_job_cell(&job, header);
                format!("{:<width$}", value, width = widths[header])
            })
            .collect();
        println!("{}", row.join(" "));
    }
}

fn display_grouped_jobs(jobs: Vec<gflow::core::job::Job>, format: Option<&str>) {
    use gflow::core::job::JobState;

    let mut grouped = std::collections::HashMap::new();
    for job in jobs {
        grouped
            .entry(job.state.clone())
            .or_insert_with(Vec::new)
            .push(job);
    }

    let states_order = [
        JobState::Running,
        JobState::Queued,
        JobState::Finished,
        JobState::Failed,
        JobState::Cancelled,
        JobState::Timeout,
    ];

    for state in states_order {
        if let Some(state_jobs) = grouped.get(&state) {
            println!("\n{} ({})", state, state_jobs.len());
            println!("{}", "─".repeat(60));
            display_jobs_table(state_jobs.clone(), format);
        }
    }
}

fn format_elapsed_time(started_at: Option<SystemTime>, finished_at: Option<SystemTime>) -> String {
    match started_at {
        Some(start_time) => {
            // For finished/failed jobs, use finished_at; for running jobs, use current time
            let end_time = finished_at.unwrap_or_else(SystemTime::now);

            if let Ok(elapsed) = end_time.duration_since(start_time) {
                let total_seconds = elapsed.as_secs();
                let days = total_seconds / 86400;
                let hours = (total_seconds % 86400) / 3600;
                let minutes = (total_seconds % 3600) / 60;
                let seconds = total_seconds % 60;

                if days > 0 {
                    format!("{}-{:02}:{:02}:{:02}", days, hours, minutes, seconds)
                } else {
                    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                }
            } else {
                "-".to_string()
            }
        }
        None => "-".to_string(),
    }
}

/// Formats a Duration as HH:MM:SS or D-HH:MM:SS
fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!("{}-{:02}:{:02}:{:02}", days, hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }
}

/// Returns the default column width for a given header
fn get_width(header: &str) -> usize {
    match header {
        "JOBID" => 8,
        "NAME" => 20,
        "ST" => 5,
        "TIME" => 12,
        "TIMELIMIT" => 12,
        "NODES" => 8,
        "NODELIST(REASON)" => 15,
        _ => 10,
    }
}

/// Formats a job field value for display
fn format_job_cell(job: &gflow::core::job::Job, header: &str) -> String {
    match header {
        "JOBID" => job.id.to_string(),
        "NAME" => job.run_name.as_deref().unwrap_or("-").to_string(),
        "ST" => job.state.short_form().to_string(),
        "NODES" => job.gpus.to_string(),
        "NODELIST(REASON)" => job.gpu_ids.as_ref().map_or_else(
            || "-".to_string(),
            |ids| {
                ids.iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        ),
        "TIME" => format_elapsed_time(job.started_at, job.finished_at),
        "TIMELIMIT" => job
            .time_limit
            .map_or_else(|| "UNLIMITED".to_string(), format_duration),
        _ => String::new(),
    }
}

/// Calculates the dynamic width for a column based on actual content
fn get_dynamic_width(header: &str, jobs: &[gflow::core::job::Job]) -> usize {
    let min_width = get_width(header);

    let content_width = match header {
        "NAME" => jobs
            .iter()
            .map(|j| j.run_name.as_deref().unwrap_or("-").len())
            .max()
            .unwrap_or(0),
        "NODELIST(REASON)" => jobs
            .iter()
            .filter_map(|j| j.gpu_ids.as_ref())
            .map(|ids| {
                // Calculate length without building string: digits + commas
                if ids.is_empty() {
                    1 // "-"
                } else {
                    ids.iter().map(|id| id.to_string().len()).sum::<usize>() + (ids.len() - 1)
                    // commas between IDs
                }
            })
            .max()
            .unwrap_or(0),
        _ => 0,
    };

    // Return the larger of min_width or content_width
    min_width.max(content_width).max(header.len())
}

/// Tree structure for dependency visualization
struct JobNode {
    job: gflow::core::job::Job,
    children: Vec<JobNode>,
}

impl JobNode {
    /// Calculates the maximum depth of the tree from this node
    fn max_depth(&self) -> usize {
        self.children
            .iter()
            .map(|c| c.max_depth() + 1)
            .max()
            .unwrap_or(0)
    }
}

/// Builds a dependency tree from a list of jobs, with cycle detection
fn build_dependency_tree(jobs: Vec<gflow::core::job::Job>) -> Vec<JobNode> {
    // Create a map of job_id -> job for quick lookup
    let job_map: HashMap<u32, gflow::core::job::Job> =
        jobs.iter().map(|j| (j.id, j.clone())).collect();

    // Create a map of parent_id -> child jobs
    let mut children_map: HashMap<Option<u32>, Vec<u32>> = HashMap::new();

    for job in &jobs {
        children_map.entry(job.depends_on).or_default().push(job.id);
    }

    // Build tree nodes recursively with cycle detection
    fn build_node(
        job_id: u32,
        job_map: &HashMap<u32, gflow::core::job::Job>,
        children_map: &HashMap<Option<u32>, Vec<u32>>,
        visited: &mut HashSet<u32>,
        recursion_stack: &mut HashSet<u32>,
    ) -> Option<JobNode> {
        // Check for circular dependency
        if recursion_stack.contains(&job_id) {
            log::warn!(
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

        let child_ids = children_map.get(&Some(job_id)).cloned().unwrap_or_default();

        let children: Vec<JobNode> = child_ids
            .into_iter()
            .filter_map(|child_id| {
                build_node(child_id, job_map, children_map, visited, recursion_stack)
            })
            .collect();

        // Remove from recursion stack (backtrack)
        recursion_stack.remove(&job_id);

        Some(JobNode { job, children })
    }

    // Find root jobs (jobs with no dependencies or dependencies not in the list)
    let root_ids = children_map.get(&None).cloned().unwrap_or_default();

    let mut visited = HashSet::new();
    let mut recursion_stack = HashSet::new();

    root_ids
        .into_iter()
        .filter_map(|job_id| {
            build_node(
                job_id,
                &job_map,
                &children_map,
                &mut visited,
                &mut recursion_stack,
            )
        })
        .collect()
}

/// Displays jobs in a tree format showing dependency relationships
fn display_jobs_tree(jobs: Vec<gflow::core::job::Job>, format: Option<&str>) {
    if jobs.is_empty() {
        println!("No jobs to display.");
        return;
    }

    let format = format
        .unwrap_or("JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)")
        .to_string();
    let headers: Vec<&str> = format.split(',').collect();

    // Build dependency tree first to calculate max depth
    let tree = build_dependency_tree(jobs.clone());

    // Calculate max depth of tree
    let max_depth = tree.iter().map(|node| node.max_depth()).max().unwrap_or(0);

    // Calculate max job ID length
    let max_id_len = jobs
        .iter()
        .map(|j| j.id.to_string().len())
        .max()
        .unwrap_or(0);

    // Calculate dynamic column widths based on actual content
    let mut widths: HashMap<&str, usize> = headers
        .iter()
        .map(|h| (*h, get_dynamic_width(h, &jobs)))
        .collect();

    // JOBID column needs space for: max_id_len + tree_chars (TREE_CHARS_PER_LEVEL per level)
    // Also ensure minimum width matches header length
    if let Some(jobid_width) = widths.get_mut("JOBID") {
        let needed_width = max_id_len + (max_depth * TREE_CHARS_PER_LEVEL);
        *jobid_width = needed_width.max("JOBID".len());
    }

    // Print header with dynamic widths
    println!(
        "{}",
        headers
            .iter()
            .map(|h| format!("{:<width$}", h, width = widths[h]))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Display each root and its children
    let root_count = tree.len();
    for (idx, node) in tree.into_iter().enumerate() {
        let is_last_root = idx == root_count - 1;
        display_job_node(&node, &headers, &widths, "", is_last_root, true);
    }
}

/// Renders a single job node and its children in tree format
fn display_job_node(
    node: &JobNode,
    headers: &[&str],
    widths: &HashMap<&str, usize>,
    prefix: &str,
    is_last: bool,
    is_root: bool,
) {
    let job = &node.job;
    let tree_prefix = if is_root {
        String::new()
    } else if is_last {
        TREE_EDGE.to_string()
    } else {
        TREE_BRANCH.to_string()
    };

    // Build the row
    let row: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(idx, header)| {
            let value = if *header == "JOBID" && idx == 0 {
                // Add tree prefix to JOBID column
                format!("{}{}{}", prefix, tree_prefix, job.id)
            } else {
                format_job_cell(job, header)
            };

            let width = widths[header];
            format!("{:<width$}", value, width = width)
        })
        .collect();

    println!("{}", row.join(" "));

    // Display children with updated prefix
    let child_count = node.children.len();
    for (idx, child) in node.children.iter().enumerate() {
        let is_last_child = idx == child_count - 1;
        // Root nodes should not add any prefix to their children
        // Non-root nodes add TREE_PIPE if not last, TREE_EMPTY if last (to maintain tree structure)
        let child_prefix = if is_root {
            String::new()
        } else if is_last {
            format!("{}{}", prefix, TREE_EMPTY)
        } else {
            format!("{}{}", prefix, TREE_PIPE)
        };

        display_job_node(child, headers, widths, &child_prefix, is_last_child, false);
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
            task_id: None,
            run_name: Some(name.to_string()),
            state: JobState::Finished,
            gpu_ids: Some(vec![0]),
            started_at: None,
            finished_at: None,
            time_limit: None,
        }
    }

    #[test]
    fn test_simple_dependency_tree() {
        let jobs = vec![
            create_test_job(1, "root-job", None),
            create_test_job(2, "child-job-1", Some(1)),
            create_test_job(3, "child-job-2", Some(1)),
        ];
        println!();
        display_jobs_tree(jobs, None);
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
        display_jobs_tree(jobs, None);
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
        display_jobs_tree(jobs, None);
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
        display_jobs_tree(jobs, None);
    }

    #[test]
    fn test_missing_parent_job() {
        let jobs = vec![
            create_test_job(1, "job-1", None),
            create_test_job(2, "job-2", Some(99)), // Parent 99 doesn't exist
            create_test_job(3, "job-3", Some(1)),
        ];
        println!();
        display_jobs_tree(jobs, None);
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
        display_jobs_tree(jobs, None);
    }

    #[test]
    fn test_empty_job_list() {
        let jobs: Vec<Job> = vec![];
        println!();
        display_jobs_tree(jobs, None);
    }

    #[test]
    fn test_tree_with_long_job_names() {
        let jobs = vec![
            create_test_job(1, "very-long-root-job-name-here", None),
            create_test_job(2, "extremely-long-child-job-name", Some(1)),
            create_test_job(3, "short", Some(1)),
        ];
        println!();
        display_jobs_tree(jobs, None);
    }

    #[test]
    fn test_max_depth_calculation() {
        let jobs = vec![
            create_test_job(1, "root", None),
            create_test_job(2, "level-1", Some(1)),
            create_test_job(3, "level-2", Some(2)),
        ];

        let tree = build_dependency_tree(jobs);
        assert_eq!(tree[0].max_depth(), 2);
    }
}
