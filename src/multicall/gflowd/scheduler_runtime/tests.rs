use super::gpu::{format_manual_ignore_reason, format_unmanaged_process_reason};
use super::*;
use gflow::core::executor::Executor;
use gflow::core::info::IgnoredGpuProcess;
use gflow::core::job::{GpuSharingMode, Job, JobState};

struct NoopExecutor;

impl Executor for NoopExecutor {
    fn execute(&self, _job: &Job) -> anyhow::Result<()> {
        Ok(())
    }
}

#[test]
fn formats_gpu_process_reasons() {
    assert_eq!(
        format_manual_ignore_reason(2, &[1234, 5678]),
        "manual_ignore(gpu=2,pid=1234,5678)"
    );
    assert_eq!(
        format_unmanaged_process_reason(&[222, 333]),
        "unmanaged(pid=222,333)"
    );
}

#[test]
fn list_ignored_gpu_processes_is_sorted() {
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        tempfile::tempdir().unwrap().path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    runtime.ignored_gpu_processes.insert(IgnoredGpuProcess {
        gpu_index: 3,
        pid: 99,
    });
    runtime.ignored_gpu_processes.insert(IgnoredGpuProcess {
        gpu_index: 1,
        pid: 200,
    });
    runtime.ignored_gpu_processes.insert(IgnoredGpuProcess {
        gpu_index: 1,
        pid: 100,
    });

    assert_eq!(
        runtime.list_ignored_gpu_processes(),
        vec![
            IgnoredGpuProcess {
                gpu_index: 1,
                pid: 100
            },
            IgnoredGpuProcess {
                gpu_index: 1,
                pid: 200
            },
            IgnoredGpuProcess {
                gpu_index: 3,
                pid: 99
            },
        ]
    );
}

#[tokio::test]
async fn rejects_whitespace_project_when_project_is_required() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig {
            known_projects: vec![],
            require_project: true,
        },
    )
    .unwrap();

    let job = Job::builder()
        .command("echo test")
        .submitted_by("alice")
        .project(Some("   ".to_string()))
        .build();

    let result = runtime.submit_job(job).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Project is required"));
    assert_eq!(runtime.next_job_id(), 1);
    assert!(runtime.get_job(1).is_none());
}

#[tokio::test]
async fn batch_project_validation_is_all_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig {
            known_projects: vec!["alpha".to_string()],
            require_project: true,
        },
    )
    .unwrap();

    let valid_job = Job::builder()
        .command("echo valid")
        .submitted_by("alice")
        .project(Some("alpha".to_string()))
        .build();
    let invalid_job = Job::builder()
        .command("echo invalid")
        .submitted_by("alice")
        .project(Some("unknown".to_string()))
        .build();

    let result = runtime.submit_jobs(vec![valid_job, invalid_job]).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown project"));
    assert_eq!(runtime.next_job_id(), 1);
    assert!(runtime.get_job(1).is_none());
}

#[tokio::test]
async fn rejects_shared_job_without_gpu_memory_limit() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let job = Job::builder()
        .command("echo test")
        .submitted_by("alice")
        .shared(true)
        .build();

    let result = runtime.submit_job(job).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Shared jobs must include a GPU memory limit"));
    assert_eq!(runtime.next_job_id(), 1);
    assert!(runtime.get_job(1).is_none());
}

#[tokio::test]
async fn normalizes_custom_run_name_for_tmux_targets() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let job = Job::builder()
        .command("echo test")
        .submitted_by("alice")
        .run_name(Some("train:v1.2".to_string()))
        .build();

    let (job_id, run_name, stored_job) = runtime.submit_job(job).await.unwrap();

    assert_eq!(job_id, 1);
    assert_eq!(run_name, "gjob-1-train_v1_2");
    assert_eq!(stored_job.run_name.as_deref(), Some("gjob-1-train_v1_2"));
}

#[tokio::test]
async fn prefixes_custom_run_names_with_job_id_to_avoid_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let job1 = Job::builder()
        .command("echo first")
        .submitted_by("alice")
        .run_name(Some("demo".to_string()))
        .build();
    let job2 = Job::builder()
        .command("echo second")
        .submitted_by("alice")
        .run_name(Some("demo".to_string()))
        .build();

    let (_, run_name1, _) = runtime.submit_job(job1).await.unwrap();
    let (_, run_name2, _) = runtime.submit_job(job2).await.unwrap();

    assert_eq!(run_name1, "gjob-1-demo");
    assert_eq!(run_name2, "gjob-2-demo");
}

#[tokio::test]
async fn batch_submit_assigns_unique_default_run_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let job1 = Job::builder()
        .command("echo first")
        .submitted_by("alice")
        .build();
    let job2 = Job::builder()
        .command("echo second")
        .submitted_by("alice")
        .build();

    let (results, submitted_jobs, next_id) = runtime.submit_jobs(vec![job1, job2]).await.unwrap();

    assert_eq!(next_id, 3);
    assert_eq!(results.len(), 2);
    assert_eq!(submitted_jobs.len(), 2);
    assert_eq!(results[0].0, 1);
    assert_eq!(results[0].1, "gjob-1");
    assert_eq!(results[1].0, 2);
    assert_eq!(results[1].1, "gjob-2");
    assert_eq!(submitted_jobs[0].run_name.as_deref(), Some("gjob-1"));
    assert_eq!(submitted_jobs[1].run_name.as_deref(), Some("gjob-2"));
}

#[tokio::test]
async fn batch_submit_assigns_unique_custom_run_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let job1 = Job::builder()
        .command("echo first")
        .submitted_by("alice")
        .run_name(Some("demo".to_string()))
        .build();
    let job2 = Job::builder()
        .command("echo second")
        .submitted_by("alice")
        .run_name(Some("demo".to_string()))
        .build();

    let (results, submitted_jobs, next_id) = runtime.submit_jobs(vec![job1, job2]).await.unwrap();

    assert_eq!(next_id, 3);
    assert_eq!(results.len(), 2);
    assert_eq!(submitted_jobs.len(), 2);
    assert_eq!(results[0].0, 1);
    assert_eq!(results[0].1, "gjob-1-demo");
    assert_eq!(results[1].0, 2);
    assert_eq!(results[1].1, "gjob-2-demo");
    assert_eq!(submitted_jobs[0].run_name.as_deref(), Some("gjob-1-demo"));
    assert_eq!(submitted_jobs[1].run_name.as_deref(), Some("gjob-2-demo"));
}

#[tokio::test]
async fn rejects_updating_shared_job_to_clear_gpu_memory_limit() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let job = Job::builder()
        .command("echo test")
        .submitted_by("alice")
        .shared(true)
        .gpu_memory_limit_mb(Some(1024))
        .build();
    let (job_id, _run_name, _job) = runtime.submit_job(job).await.unwrap();

    let req = crate::multicall::gflowd::server::UpdateJobRequest {
        command: None,
        script: None,
        gpus: None,
        conda_env: None,
        priority: None,
        parameters: None,
        time_limit: None,
        memory_limit_mb: None,
        gpu_memory_limit_mb: Some(None),
        depends_on_ids: None,
        dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        max_concurrent: None,
        max_retries: None,
        notifications: None,
    };

    let result = runtime.update_job(job_id, req).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("Shared jobs must keep a GPU memory limit"));

    let current = runtime.get_job(job_id).unwrap();
    assert_eq!(current.gpu_sharing_mode, GpuSharingMode::Shared);
    assert_eq!(current.gpu_memory_limit_mb, Some(1024));
}

#[tokio::test]
async fn updates_job_notifications() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let job = Job::builder()
        .command("echo test")
        .submitted_by("alice")
        .build();
    let (job_id, _run_name, _job) = runtime.submit_job(job).await.unwrap();

    let req = crate::multicall::gflowd::server::UpdateJobRequest {
        command: None,
        script: None,
        gpus: None,
        conda_env: None,
        priority: None,
        parameters: None,
        time_limit: None,
        memory_limit_mb: None,
        gpu_memory_limit_mb: None,
        depends_on_ids: None,
        dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        max_concurrent: None,
        max_retries: None,
        notifications: Some(gflow::core::job::JobNotifications::normalized(
            vec!["alice@example.com".to_string()],
            vec!["job_failed".to_string()],
        )),
    };

    let (updated, updated_fields) = runtime.update_job(job_id, req).await.unwrap();

    assert_eq!(updated_fields, vec!["notifications".to_string()]);
    assert_eq!(updated.notifications.emails.len(), 1);
    assert_eq!(
        updated.notifications.emails[0].as_str(),
        "alice@example.com"
    );
    assert_eq!(
        updated
            .notifications
            .events
            .iter()
            .map(|event| event.as_str())
            .collect::<Vec<_>>(),
        vec!["job_failed"]
    );
}

#[tokio::test]
async fn fail_job_creates_retry_attempt_and_retargets_queued_dependents() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let root = Job::builder()
        .command("echo root")
        .submitted_by("alice")
        .max_retries(2)
        .build();
    let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

    let child = Job::builder()
        .command("echo child")
        .submitted_by("alice")
        .depends_on(Some(root_id))
        .depends_on_ids(vec![root_id])
        .build();
    let (child_id, _run_name, _job) = runtime.submit_job(child).await.unwrap();

    let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
    assert_eq!(jobs_to_execute.len(), 1);
    assert_eq!(jobs_to_execute[0].id, root_id);
    assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Running);

    let retry_result = runtime.fail_job(root_id).await;
    assert_eq!(retry_result, Some(Some(3)));

    let failed_root = runtime.get_job(root_id).unwrap();
    assert_eq!(failed_root.state, JobState::Failed);

    let retry_job = runtime.get_job(3).unwrap();
    assert_eq!(retry_job.redone_from, Some(root_id));
    assert_eq!(retry_job.max_retries, 2);
    assert_eq!(retry_job.state, JobState::Queued);

    let updated_child = runtime.get_job(child_id).unwrap();
    assert_eq!(updated_child.state, JobState::Queued);
    assert_eq!(updated_child.all_dependency_ids().as_slice(), &[3]);
}

#[tokio::test]
async fn explicit_fail_job_does_not_spawn_retry_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let root = Job::builder()
        .command("echo root")
        .submitted_by("alice")
        .max_retries(2)
        .build();
    let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

    let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
    assert_eq!(jobs_to_execute.len(), 1);
    assert_eq!(jobs_to_execute[0].id, root_id);

    assert!(runtime.explicit_fail_job(root_id).await);
    assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Failed);
    assert!(runtime.get_job(2).is_none());
}

#[tokio::test]
async fn manual_redo_lineage_does_not_consume_automatic_retry_budget() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let root = Job::builder()
        .command("echo root")
        .submitted_by("alice")
        .build();
    let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

    let manual_redo = Job::builder()
        .command("echo redo")
        .submitted_by("alice")
        .redone_from(Some(root_id))
        .max_retries(1)
        .build();
    let (redo_id, _run_name, _job) = runtime.submit_job(manual_redo).await.unwrap();

    let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
    assert_eq!(jobs_to_execute.len(), 2);
    assert!(jobs_to_execute.iter().any(|job| job.id == redo_id));

    let retry_result = runtime.fail_job(redo_id).await;
    assert_eq!(retry_result, Some(Some(3)));

    let retry_job = runtime.get_job(3).unwrap();
    assert_eq!(retry_job.redone_from, Some(root_id));
    assert_eq!(retry_job.retried_from, Some(redo_id));
}

#[tokio::test]
async fn sibling_manual_redos_do_not_share_automatic_retry_budget() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let root = Job::builder()
        .command("echo root")
        .submitted_by("alice")
        .build();
    let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

    let redo_a = Job::builder()
        .command("echo redo a")
        .submitted_by("alice")
        .redone_from(Some(root_id))
        .max_retries(1)
        .build();
    let (redo_a_id, _run_name, _job) = runtime.submit_job(redo_a).await.unwrap();

    let redo_b = Job::builder()
        .command("echo redo b")
        .submitted_by("alice")
        .redone_from(Some(root_id))
        .max_retries(1)
        .build();
    let (redo_b_id, _run_name, _job) = runtime.submit_job(redo_b).await.unwrap();

    let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
    assert!(jobs_to_execute.iter().any(|job| job.id == redo_a_id));
    assert!(jobs_to_execute.iter().any(|job| job.id == redo_b_id));

    assert_eq!(runtime.fail_job(redo_a_id).await, Some(Some(4)));
    assert_eq!(runtime.fail_job(redo_b_id).await, Some(Some(5)));

    let retry_a = runtime.get_job(4).unwrap();
    assert_eq!(retry_a.redone_from, Some(root_id));
    assert_eq!(retry_a.retried_from, Some(redo_a_id));

    let retry_b = runtime.get_job(5).unwrap();
    assert_eq!(retry_b.redone_from, Some(root_id));
    assert_eq!(retry_b.retried_from, Some(redo_b_id));
}

#[tokio::test]
async fn timeout_does_not_spawn_retry_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    let root = Job::builder()
        .command("echo root")
        .submitted_by("alice")
        .max_retries(2)
        .build();
    let (root_id, _run_name, _job) = runtime.submit_job(root).await.unwrap();

    let jobs_to_execute = runtime.scheduler.prepare_jobs_for_execution();
    assert_eq!(jobs_to_execute.len(), 1);
    assert_eq!(jobs_to_execute[0].id, root_id);
    assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Running);

    let timeout_result = runtime.timeout_job(root_id).await;
    assert_eq!(timeout_result, Some(None));
    assert_eq!(runtime.get_job(root_id).unwrap().state, JobState::Timeout);
    assert!(runtime.get_job(2).is_none());
}

#[tokio::test]
async fn enters_journal_mode_and_does_not_overwrite_state_on_migration_failure() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.json");

    // Use a future version to force `migrate_state()` to fail.
    let state_json = serde_json::json!({
        "version": 999,
        "jobs": [
            {
                "id": 1,
                "state": "Queued",
                "script": null,
                "command": "echo test",
                "gpus": 0,
                "conda_env": null,
                "run_dir": ".",
                "priority": 0,
                "depends_on": null,
                "depends_on_ids": [],
                "dependency_mode": null,
                "auto_cancel_on_dependency_failure": true,
                "task_id": null,
                "time_limit": null,
                "memory_limit_mb": null,
                "submitted_by": "tester",
                "redone_from": null,
                "auto_close_tmux": false,
                "parameters": {},
                "group_id": null,
                "max_concurrent": null,
                "run_name": null,
                "gpu_ids": null,
                "submitted_at": null,
                "started_at": null,
                "finished_at": null,
                "reason": null
            }
        ],
        "state_path": "state.json",
        "next_job_id": 2,
        "allowed_gpu_indices": null
    })
    .to_string();
    std::fs::write(&state_path, &state_json).unwrap();
    let original = std::fs::read_to_string(&state_path).unwrap();

    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    assert!(!runtime.state_writable());
    assert!(runtime.state_load_error().is_some());
    assert!(runtime.state_backup_path().is_some_and(|p| p.exists()));
    assert!(runtime.journal_writable());
    assert_eq!(runtime.persistence_mode(), "journal");

    // State is still visible for inspection.
    let job = runtime.get_job(1).unwrap();
    assert_eq!(job.state, JobState::Queued);

    // `save_state()` should append to journal and not overwrite the original file.
    runtime.save_state().await;
    let after = std::fs::read_to_string(&state_path).unwrap();
    assert_eq!(after, original);

    let journal_path = dir.path().join("state.journal.jsonl");
    let journal = std::fs::read_to_string(&journal_path).unwrap();
    assert!(journal.contains("\"kind\":\"snapshot\""));
    assert!(journal.contains("\"jobs\""));

    // Sanity: scheduler is still usable for read paths (no panic on info).
    let _info = runtime.info();
}

#[tokio::test]
async fn prefers_newer_journal_snapshot_and_truncates_after_state_save() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    let journal_path = dir.path().join("state.journal.jsonl");

    let job = serde_json::json!({
        "id": 1,
        "state": "Queued",
        "script": null,
        "command": "echo test",
        "gpus": 0,
        "conda_env": null,
        "run_dir": ".",
        "priority": 0,
        "depends_on": null,
        "depends_on_ids": [],
        "dependency_mode": null,
        "auto_cancel_on_dependency_failure": true,
        "task_id": null,
        "time_limit": null,
        "memory_limit_mb": null,
        "submitted_by": "tester",
        "redone_from": null,
        "auto_close_tmux": false,
        "parameters": {},
        "group_id": null,
        "max_concurrent": null,
        "run_name": null,
        "gpu_ids": null,
        "submitted_at": null,
        "started_at": null,
        "finished_at": null,
        "reason": null
    });

    let state_json = serde_json::json!({
        "version": gflow::core::migrations::CURRENT_VERSION,
        "jobs": [ job ],
        "state_path": "state.json",
        "next_job_id": 2,
        "allowed_gpu_indices": null
    })
    .to_string();
    std::fs::write(&state_path, &state_json).unwrap();

    // Journal snapshot shows the job as Finished.
    let mut finished_job = serde_json::json!(job);
    finished_job["state"] = serde_json::Value::String("Finished".to_string());
    let journal_entry = serde_json::json!({
        "ts": 9999999999u64,
        "kind": "snapshot",
        "scheduler": {
            "version": gflow::core::migrations::CURRENT_VERSION,
            "jobs": [ finished_job ],
            "state_path": "state.json",
            "next_job_id": 2,
            "allowed_gpu_indices": null
        }
    })
    .to_string();
    std::fs::write(&journal_path, format!("{journal_entry}\n")).unwrap();

    let mut runtime = SchedulerRuntime::with_state_path(
        Box::new(NoopExecutor),
        dir.path().to_path_buf(),
        None,
        gflow::core::gpu_allocation::GpuAllocationStrategy::Sequential,
        gflow::config::ProjectsConfig::default(),
    )
    .unwrap();

    assert_eq!(runtime.persistence_mode(), "state");
    assert_eq!(runtime.get_job(1).unwrap().state, JobState::Finished);

    // load_state marked the runtime dirty, so this should consolidate into state.json and truncate the journal.
    runtime.save_state_if_dirty().await;

    let journal_after = std::fs::read_to_string(&journal_path).unwrap();
    assert!(journal_after.trim().is_empty());

    // State is now saved in MessagePack format
    let msgpack_path = dir.path().join("state.msgpack");
    assert!(msgpack_path.exists(), "state.msgpack should exist");

    // Verify the state was saved correctly by loading it back
    let state_bytes = std::fs::read(&msgpack_path).unwrap();
    let loaded_scheduler: Scheduler = rmp_serde::from_slice(&state_bytes).unwrap();
    assert_eq!(
        loaded_scheduler.get_job(1).unwrap().state,
        JobState::Finished
    );
}
