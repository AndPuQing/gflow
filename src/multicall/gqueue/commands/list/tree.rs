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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RelationshipKind {
    Dependency,
    Redo,
}

/// Represents a child in the tree - either a real job node or a reference
pub(super) enum JobNodeChild {
    Node(Box<JobNode>, RelationshipKind),
    Reference(u32, RelationshipKind),
}

/// Context for rendering jobs with formatting and session information
struct RenderContext<'a> {
    headers: &'a [&'a str],
    tmux_sessions: &'a HashSet<String>,
}

#[derive(Clone, Copy)]
struct ChildRelation {
    relationship: RelationshipKind,
    reference: bool,
}

fn relation_priority(relation: ChildRelation) -> u8 {
    match (relation.reference, relation.relationship) {
        (false, RelationshipKind::Dependency) => 4,
        (false, RelationshipKind::Redo) => 3,
        (true, RelationshipKind::Dependency) => 2,
        (true, RelationshipKind::Redo) => 1,
    }
}

fn insert_child_relation(
    children_by_parent: &mut HashMap<u32, HashMap<u32, ChildRelation>>,
    parent_id: u32,
    child_id: u32,
    relation: ChildRelation,
) {
    let parent_children = children_by_parent.entry(parent_id).or_default();
    match parent_children.get_mut(&child_id) {
        Some(existing) if relation_priority(relation) > relation_priority(*existing) => {
            *existing = relation;
        }
        Some(_) => {}
        None => {
            parent_children.insert(child_id, relation);
        }
    }
}

/// Builds a dependency tree from a list of jobs, with cycle detection
pub(super) fn build_dependency_tree(jobs: &[gflow::core::job::Job]) -> Vec<JobNode> {
    // Create a map of job_id -> job for quick lookup
    let job_map: HashMap<u32, &gflow::core::job::Job> = jobs.iter().map(|j| (j.id, j)).collect();

    // Create a map of parent_id -> child jobs, annotating whether the link should
    // render as a full node or as a reference, and whether it is a dependency or redo edge.
    let mut children_by_parent: HashMap<u32, HashMap<u32, ChildRelation>> = HashMap::new();

    for job in jobs {
        let present_dependency_parents: Vec<u32> = job
            .dependency_ids_iter()
            .filter(|parent_id| job_map.contains_key(parent_id))
            .collect();

        if let Some((primary_parent, extra_parents)) = present_dependency_parents.split_first() {
            insert_child_relation(
                &mut children_by_parent,
                *primary_parent,
                job.id,
                ChildRelation {
                    relationship: RelationshipKind::Dependency,
                    reference: false,
                },
            );

            for parent_id in extra_parents {
                insert_child_relation(
                    &mut children_by_parent,
                    *parent_id,
                    job.id,
                    ChildRelation {
                        relationship: RelationshipKind::Dependency,
                        reference: true,
                    },
                );
            }
        }

        if let Some(redone_from) = job
            .redone_from
            .filter(|parent_id| job_map.contains_key(parent_id))
        {
            insert_child_relation(
                &mut children_by_parent,
                redone_from,
                job.id,
                ChildRelation {
                    relationship: RelationshipKind::Redo,
                    reference: !present_dependency_parents.is_empty(),
                },
            );
        }
    }

    // Build tree nodes recursively with cycle detection
    fn build_node(
        job_id: u32,
        job_map: &HashMap<u32, &gflow::core::job::Job>,
        children_by_parent: &HashMap<u32, HashMap<u32, ChildRelation>>,
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

        let mut children: Vec<JobNodeChild> = children_by_parent
            .get(&job_id)
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|(&child_id, &relation)| {
                if relation.reference {
                    Some(JobNodeChild::Reference(child_id, relation.relationship))
                } else {
                    build_node(
                        child_id,
                        job_map,
                        children_by_parent,
                        visited,
                        recursion_stack,
                    )
                    .map(|child_node| {
                        JobNodeChild::Node(Box::new(child_node), relation.relationship)
                    })
                }
            })
            .collect();

        // Sort children by job ID to maintain proper ordering
        children.sort_by_key(|child| match child {
            JobNodeChild::Node(node, _) => node.job.id,
            JobNodeChild::Reference(id, _) => *id,
        });

        // Remove from recursion stack (backtrack)
        recursion_stack.remove(&job_id);

        Some(JobNode { job, children })
    }

    // Find root jobs:
    // - jobs with no dependency
    // - jobs whose dependency and redo parents are outside the current filtered job list
    let mut root_ids: Vec<u32> = jobs
        .iter()
        .filter(|job| {
            let has_present_dependency_parent = job
                .dependency_ids_iter()
                .any(|parent_id| job_map.contains_key(&parent_id));
            let has_present_redo_parent = job
                .redone_from
                .is_some_and(|parent_id| job_map.contains_key(&parent_id));

            !has_present_dependency_parent && !has_present_redo_parent
        })
        .map(|job| job.id)
        .collect();

    // Keep root ordering deterministic by job id.
    root_ids.sort_unstable();
    root_ids.dedup();

    let mut visited = HashSet::new();
    let mut recursion_stack = HashSet::new();

    root_ids
        .into_iter()
        .filter_map(|job_id| {
            build_node(
                job_id,
                &job_map,
                &children_by_parent,
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
        collect_tree_rows(
            &mut builder,
            node,
            &ctx,
            "",
            true,
            true,
            RelationshipKind::Dependency,
        );
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
    relationship: RelationshipKind,
) {
    let job = &node.job;
    let tree_prefix = if is_root {
        String::new()
    } else if relationship == RelationshipKind::Redo {
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
            JobNodeChild::Node(child_node, child_relationship) => {
                collect_tree_rows(
                    builder,
                    child_node,
                    ctx,
                    &child_prefix,
                    is_last_child,
                    false,
                    *child_relationship,
                );
            }
            JobNodeChild::Reference(job_id, child_relationship) => {
                // Add a reference row - make it compact by using minimal spacing
                let tree_prefix = if *child_relationship == RelationshipKind::Dependency {
                    if is_last_child {
                        TREE_EDGE
                    } else {
                        TREE_BRANCH
                    }
                } else if is_last_child {
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
