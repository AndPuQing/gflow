use super::display::format_job_cell;
use std::collections::{HashMap, HashSet};
use tabled::{builder::Builder, settings::style::Style};

const TREE_BRANCH: &str = "├─";
const TREE_EDGE: &str = "╰─";
const TREE_PIPE: &str = "│ ";
const TREE_EMPTY: &str = "  ";

// Tree rendering constants - dashed lines for redo relationships
const TREE_BRANCH_DASHED: &str = "├┄";
const TREE_EDGE_DASHED: &str = "╰┄";

pub(super) struct JobNode {
    pub(super) job: gflow::core::job::Job,
    pub(super) children: Vec<JobNodeChild>,
}

/// Represents a child in the tree - either a real job node or a reference
pub(super) enum JobNodeChild {
    Node(Box<JobNode>, bool), // (node, is_redo_relationship)
    Reference(u32),           // Reference to a job ID that appears elsewhere
}

/// Context for rendering jobs with formatting and session information
struct RenderContext<'a> {
    headers: &'a [&'a str],
    tmux_sessions: &'a HashSet<String>,
}

/// Builds a dependency tree from a list of jobs, with cycle detection
pub(super) fn build_dependency_tree(jobs: &[gflow::core::job::Job]) -> Vec<JobNode> {
    // Create a map of job_id -> job for quick lookup
    let job_map: HashMap<u32, &gflow::core::job::Job> = jobs.iter().map(|j| (j.id, j)).collect();

    // Create a map of parent_id -> child jobs (for dependency relationships)
    let mut children_map: HashMap<Option<u32>, Vec<u32>> = HashMap::new();

    // Create a map of original_job_id -> redo jobs (for redo relationships)
    let mut redo_map: HashMap<u32, Vec<u32>> = HashMap::new();

    // Track all jobs that appear as dependency children (globally)
    let mut all_dependency_children: HashSet<u32> = HashSet::new();

    for job in jobs {
        children_map.entry(job.depends_on).or_default().push(job.id);

        // Track jobs whose dependency parent is present in the current list.
        // If the dependency parent is filtered out, this job should still be rendered normally.
        if let Some(parent_id) = job.depends_on {
            if job_map.contains_key(&parent_id) {
                all_dependency_children.insert(job.id);
            }
        }

        if let Some(redone_from) = job.redone_from {
            redo_map.entry(redone_from).or_default().push(job.id);
        }
    }

    // Build tree nodes recursively with cycle detection
    fn build_node(
        job_id: u32,
        job_map: &HashMap<u32, &gflow::core::job::Job>,
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
        let job = (*job_map.get(&job_id)?).clone();

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

    // Find root jobs:
    // - jobs with no dependency
    // - jobs whose dependency parent is outside the current filtered job list
    let mut root_ids: Vec<u32> = jobs
        .iter()
        .filter_map(|job| match job.depends_on {
            None => Some(job.id),
            Some(parent_id) if !job_map.contains_key(&parent_id) => Some(job.id),
            _ => None,
        })
        .collect();

    // Keep root ordering deterministic by job id.
    root_ids.sort_unstable();
    root_ids.dedup();

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
pub(super) fn display_jobs_tree(
    jobs: &[gflow::core::job::Job],
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
    let tree = build_dependency_tree(jobs);

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
