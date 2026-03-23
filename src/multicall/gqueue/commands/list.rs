use anyhow::Result;
use gflow::{client::Client, core::job::JobState, tmux::get_all_session_names};

mod display;
mod output;
mod tree;

use display::{display_grouped_jobs, display_jobs_table};
use output::{output_csv, output_json, output_yaml, OutputFormat};
#[cfg(test)]
use std::collections::HashSet;
use tree::display_jobs_tree;
#[cfg(test)]
use tree::{build_dependency_tree, JobNodeChild};

pub struct ListOptions {
    pub user: Option<String>,
    pub states: Option<String>,
    pub jobs: Option<String>,
    pub names: Option<String>,
    pub project: Option<String>,
    pub sort: String,
    pub limit: i32,
    pub all: bool,
    pub completed: bool,
    pub since: Option<String>,
    pub group: bool,
    pub tree: bool,
    pub format: Option<String>,
    pub tmux: bool,
    pub output: String,
    pub watch: bool,
    pub interval: u64,
}

pub async fn handle_list(client: &Client, options: ListOptions) -> Result<()> {
    if options.watch {
        let interval = std::time::Duration::from_secs(options.interval);
        loop {
            print!("\x1B[2J\x1B[H");
            let now = chrono::Local::now();
            println!(
                "Last updated: {}  [Refreshing every {}s. Press Ctrl+C to exit]\n",
                now.format("%Y-%m-%d %H:%M:%S"),
                options.interval
            );
            display_once(client, &options).await?;
            tokio::time::sleep(interval).await;
        }
    } else {
        display_once(client, &options).await
    }
}

async fn display_once(client: &Client, options: &ListOptions) -> Result<()> {
    let current_user = gflow::platform::get_current_username();
    let user_filter = match options.user.as_deref().map(str::trim) {
        None => Some(current_user.clone()),
        Some("") => Some(current_user.clone()),
        Some("all") | Some("*") => None,
        Some(u) => Some(u.to_string()),
    };

    let states_filter = if options.completed {
        Some(
            JobState::completed_states()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    } else if let Some(ref states) = options.states {
        Some(states.clone())
    } else if options.all {
        None
    } else {
        Some(
            JobState::active_states()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    };

    let created_after = if let Some(ref since_str) = options.since {
        Some(gflow::utils::parse_since_time(since_str)?)
    } else {
        None
    };

    let mut jobs_vec = client
        .list_jobs_with_query(states_filter, user_filter, None, None, created_after, None)
        .await?;

    if let Some(job_ids) = options.jobs.as_deref() {
        let job_ids_vec: Vec<u32> = job_ids
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !job_ids_vec.is_empty() {
            jobs_vec.retain(|job| job_ids_vec.contains(&job.id));
        }
    }

    if let Some(names_filter) = options.names.as_deref() {
        let names_vec: Vec<String> = names_filter
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        if !names_vec.is_empty() {
            jobs_vec.retain(|job| {
                job.run_name
                    .as_ref()
                    .is_some_and(|run_name| names_vec.iter().any(|n| n == run_name.as_str()))
            });
        }
    }

    if let Some(project_filter) = options.project.as_deref() {
        let project = project_filter.trim();
        if !project.is_empty() {
            jobs_vec.retain(|job| job.project.as_ref().is_some_and(|p| p.as_str() == project));
        }
    }

    let tmux_sessions = get_all_session_names();

    if options.tmux {
        jobs_vec.retain(|job| {
            job.run_name
                .as_ref()
                .is_some_and(|run_name| tmux_sessions.contains(run_name.as_str()))
        });
    }

    if jobs_vec.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    sort_jobs(&mut jobs_vec, &options.sort);

    let output_format: OutputFormat = options.output.parse().map_err(|_| {
        anyhow::anyhow!(
            "Invalid output format '{}'. Valid options: table, json, csv, yaml",
            options.output
        )
    })?;

    let effective_limit = if options.all { 0 } else { options.limit };
    let mut limit_message = None;
    if effective_limit != 0 {
        let total_jobs = jobs_vec.len();

        if effective_limit > 0 {
            let limit_usize = effective_limit as usize;
            if jobs_vec.len() > limit_usize {
                jobs_vec.truncate(limit_usize);
                limit_message = Some(format!(
                    "Showing first {} of {} jobs (use --all or -n 0 to show all)",
                    effective_limit, total_jobs
                ));
            }
        } else {
            let limit_usize = (-effective_limit) as usize;
            if jobs_vec.len() > limit_usize {
                let start = jobs_vec.len() - limit_usize;
                jobs_vec = jobs_vec.into_iter().skip(start).collect();
                limit_message = Some(format!(
                    "Showing last {} of {} jobs (use --all or -n 0 to show all)",
                    limit_usize, total_jobs
                ));
            }
        }
    }

    if let Some(msg) = limit_message {
        if output_format == OutputFormat::Table {
            println!("{}", msg);
            println!();
        }
    }

    match output_format {
        OutputFormat::Table => {
            if options.group {
                display_grouped_jobs(&jobs_vec, options.format.as_deref(), &tmux_sessions);
            } else if options.tree {
                display_jobs_tree(&jobs_vec, options.format.as_deref(), &tmux_sessions);
            } else {
                display_jobs_table(&jobs_vec, options.format.as_deref(), &tmux_sessions);
            }
        }
        OutputFormat::Json => output_json(&jobs_vec)?,
        OutputFormat::Csv => output_csv(&jobs_vec)?,
        OutputFormat::Yaml => output_yaml(&jobs_vec)?,
    }

    Ok(())
}

fn sort_jobs(jobs: &mut [gflow::core::job::Job], sort_field: &str) {
    match sort_field.to_lowercase().as_str() {
        "id" => jobs.sort_by_key(|j| j.id),
        "state" => jobs.sort_by_key(|j| j.state),
        "time" => jobs.sort_by_key(|a| a.started_at),
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

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::{GpuSharingMode, Job, JobState};
    use std::path::PathBuf;

    fn create_test_job(id: u32, name: &str, depends_on: Option<u32>) -> Job {
        Job {
            id,
            script: None,
            command: Some(format!("test command {}", id).into()),
            gpus: 1,
            conda_env: None,
            run_dir: PathBuf::from("/tmp"),
            priority: 10,
            depends_on,
            depends_on_ids: smallvec::smallvec![],
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: None,
            gpu_sharing_mode: GpuSharingMode::Exclusive,
            run_name: Some(name.into()),
            project: None,
            notifications: gflow::core::job::JobNotifications::default(),
            state: JobState::Finished,
            gpu_ids: Some(smallvec::smallvec![0]),
            submitted_at: None,
            started_at: None,
            finished_at: None,
            time_limit: None,
            memory_limit_mb: None,
            gpu_memory_limit_mb: None,
            submitted_by: "testuser".into(),
            redone_from: None,
            auto_close_tmux: false,
            parameters: gflow::core::job::Parameters::new(),
            group_id: None,
            max_concurrent: None,
            reason: None,
        }
    }

    fn create_test_job_with_dependencies(
        id: u32,
        name: &str,
        depends_on: Option<u32>,
        depends_on_ids: &[u32],
    ) -> Job {
        let mut job = create_test_job(id, name, depends_on);
        job.depends_on_ids = depends_on_ids.iter().copied().collect();
        job
    }

    fn create_test_job_with_state(id: u32, name: &str, state: JobState) -> Job {
        Job {
            id,
            script: None,
            command: Some(format!("test command {}", id).into()),
            gpus: 1,
            conda_env: None,
            run_dir: PathBuf::from("/tmp"),
            priority: 10,
            depends_on: None,
            depends_on_ids: smallvec::smallvec![],
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: None,
            gpu_sharing_mode: GpuSharingMode::Exclusive,
            run_name: Some(name.into()),
            project: None,
            notifications: gflow::core::job::JobNotifications::default(),
            state,
            gpu_ids: Some(smallvec::smallvec![0]),
            submitted_at: None,
            started_at: None,
            finished_at: None,
            time_limit: None,
            memory_limit_mb: None,
            gpu_memory_limit_mb: None,
            submitted_by: "testuser".into(),
            redone_from: None,
            auto_close_tmux: false,
            parameters: gflow::core::job::Parameters::new(),
            group_id: None,
            max_concurrent: None,
            reason: None,
        }
    }

    fn create_test_job_with_redo(id: u32, name: &str, redone_from: Option<u32>) -> Job {
        Job {
            id,
            script: None,
            command: Some(format!("test command {}", id).into()),
            gpus: 1,
            conda_env: None,
            run_dir: PathBuf::from("/tmp"),
            priority: 10,
            depends_on: None,
            depends_on_ids: smallvec::smallvec![],
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: None,
            gpu_sharing_mode: GpuSharingMode::Exclusive,
            run_name: Some(name.into()),
            project: None,
            notifications: gflow::core::job::JobNotifications::default(),
            state: JobState::Finished,
            gpu_ids: Some(smallvec::smallvec![0]),
            submitted_at: None,
            started_at: None,
            finished_at: None,
            time_limit: None,
            memory_limit_mb: None,
            gpu_memory_limit_mb: None,
            submitted_by: "testuser".into(),
            redone_from,
            auto_close_tmux: false,
            parameters: gflow::core::job::Parameters::new(),
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
        display_jobs_tree(&jobs, None, &HashSet::new());
    }

    #[test]
    fn test_simple_dependency_tree() {
        let jobs = vec![
            create_test_job(1, "root-job", None),
            create_test_job(2, "child-job-1", Some(1)),
            create_test_job(3, "child-job-2", Some(1)),
        ];
        println!();
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
    }

    #[test]
    fn test_missing_parent_job() {
        let jobs = vec![
            create_test_job(1, "job-1", None),
            create_test_job(2, "job-2", Some(99)), // Parent 99 doesn't exist
            create_test_job(3, "job-3", Some(1)),
        ];
        println!();
        display_jobs_tree(&jobs, None, &HashSet::new());
    }

    #[test]
    fn test_build_tree_treats_missing_dependency_parent_as_root() {
        let jobs = vec![
            create_test_job(1, "root", None),
            create_test_job(2, "missing-parent", Some(99)),
            create_test_job(3, "child-of-1", Some(1)),
        ];

        let tree = build_dependency_tree(&jobs);
        let root_ids: Vec<u32> = tree.iter().map(|node| node.job.id).collect();
        assert_eq!(root_ids, vec![1, 2]);

        let first_children: Vec<u32> = tree[0]
            .children
            .iter()
            .filter_map(|child| match child {
                JobNodeChild::Node(node, _) => Some(node.job.id),
                JobNodeChild::Reference(_, _) => None,
            })
            .collect();
        assert_eq!(first_children, vec![3]);
    }

    #[test]
    fn test_build_tree_keeps_node_when_dependency_parent_missing_but_redo_parent_exists() {
        let mut redo_with_missing_dep = create_test_job(2, "redo-with-missing-dep", Some(99));
        redo_with_missing_dep.redone_from = Some(1);

        let jobs = vec![create_test_job(1, "root", None), redo_with_missing_dep];

        let tree = build_dependency_tree(&jobs);
        let root_ids: Vec<u32> = tree.iter().map(|node| node.job.id).collect();
        assert_eq!(root_ids, vec![1]);

        let first_children: Vec<u32> = tree[0]
            .children
            .iter()
            .filter_map(|child| match child {
                JobNodeChild::Node(node, _) => Some(node.job.id),
                JobNodeChild::Reference(_, _) => None,
            })
            .collect();
        assert_eq!(first_children, vec![2]);
    }

    #[test]
    fn test_build_tree_supports_multi_dependencies_with_reference_parents() {
        let jobs = vec![
            create_test_job(1, "root-a", None),
            create_test_job(2, "root-b", None),
            create_test_job_with_dependencies(3, "multi-dep", None, &[1, 2]),
        ];

        let tree = build_dependency_tree(&jobs);
        let root_ids: Vec<u32> = tree.iter().map(|node| node.job.id).collect();
        assert_eq!(root_ids, vec![1, 2]);

        match &tree[0].children[..] {
            [JobNodeChild::Node(node, _)] => assert_eq!(node.job.id, 3),
            _ => panic!("expected job 3 as a concrete child of the first dependency parent"),
        }

        match &tree[1].children[..] {
            [JobNodeChild::Reference(3, _)] => {}
            _ => panic!("expected job 3 as a reference under the secondary dependency parent"),
        }
    }

    #[test]
    fn test_build_tree_uses_present_dependency_when_legacy_parent_missing() {
        let jobs = vec![
            create_test_job(1, "root", None),
            create_test_job_with_dependencies(2, "child", Some(99), &[1]),
        ];

        let tree = build_dependency_tree(&jobs);
        let root_ids: Vec<u32> = tree.iter().map(|node| node.job.id).collect();
        assert_eq!(root_ids, vec![1]);

        match &tree[0].children[..] {
            [JobNodeChild::Node(node, _)] => assert_eq!(node.job.id, 2),
            _ => panic!("expected job 2 to attach to the present dependency parent"),
        }
    }

    #[test]
    fn test_gap_job() {
        let jobs = vec![
            create_test_job(1, "job-1", None),
            create_test_job(2, "job-2", None), // Parent 99 doesn't exist
            create_test_job(3, "job-3", Some(1)),
        ];
        println!();
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
    }

    #[test]
    fn test_empty_job_list() {
        let jobs: Vec<Job> = vec![];
        println!();
        display_jobs_tree(&jobs, None, &HashSet::new());
    }

    #[test]
    fn test_tree_with_long_job_names() {
        let jobs = vec![
            create_test_job(1, "very-long-root-job-name-here", None),
            create_test_job(2, "extremely-long-child-job-name", Some(1)),
            create_test_job(3, "short", Some(1)),
        ];
        println!();
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
    }

    #[test]
    fn test_job_with_both_dependency_and_redo() {
        // This test case matches the user's scenario:
        // Job 165 has both depends_on=163 and redone_from=164
        // It should only appear once (as a dependency child) with a reference indicator
        let mut job_165 = create_test_job(165, "gjob-165", Some(163));
        job_165.redone_from = Some(164);

        let jobs = vec![
            create_test_job(162, "gjob-162", None),
            create_test_job(163, "gjob-163", Some(162)),
            create_test_job(164, "gjob-164", Some(163)),
            job_165,
        ];
        println!();
        println!("Test: Job with both dependency and redo relationship (user's scenario)");
        println!("Job 165 depends on 163 AND is a redo of 164");
        println!("Expected: Job 165 appears once under 163, with '→ see job 165 below' reference under 164");
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
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
        display_jobs_tree(&jobs, None, &HashSet::new());
    }
}
