use super::list_jobs::*;
use super::log::*;
use super::queue_pressure::*;
use super::schemas::*;
use super::submit::*;
use super::triage::*;
use super::update::*;
use super::GflowMcpServer;
use compact_str::CompactString;
use gflow::client::UpdateJobRequest;
use gflow::core::gpu_allocation::GpuAllocationStrategy;
use gflow::core::info::{GpuInfo, SchedulerInfo};
use gflow::core::job::{JobBuilder, JobState, JobStateReason};
use gflow::core::reservation::{GpuReservation, GpuSpec, ReservationStatus};
use schemars::schema_for;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[test]
fn tool_schemas_are_exposed_for_object_outputs() {
    let tools = GflowMcpServer::tool_router().list_all();

    for tool_name in [
        "get_info",
        "get_health",
        "get_job",
        "get_job_log",
        "get_stats",
        "get_queue_pressure",
        "cancel_job",
        "hold_job",
        "release_job",
        "preview_submit_jobs",
        "submit_jobs",
        "preview_update_job",
        "update_job",
        "triage_job",
        "redo_job",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == tool_name)
            .unwrap_or_else(|| panic!("missing tool: {tool_name}"));
        assert!(
            tool.output_schema.is_some(),
            "expected output schema for {tool_name}"
        );
    }
}

#[test]
fn submit_job_validation_rejects_shared_jobs_without_gpu_memory_limit() {
    let err = build_submit_job(SubmitJobRequest {
        command: Some("echo hello".to_string()),
        script: None,
        gpus: Some(1),
        conda_env: None,
        run_dir: None,
        priority: None,
        depends_on: None,
        depends_on_ids: None,
        dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        shared: Some(true),
        gpu_memory_limit_mb: None,
        time_limit_secs: None,
        memory_limit_mb: None,
        submitted_by: None,
        param: None,
        parameters: None,
        run_name: None,
        project: None,
        max_concurrent: None,
        max_retries: None,
        auto_close_tmux: None,
        notify_email: None,
        notify_on: None,
    })
    .unwrap_err();

    assert_eq!(
        err,
        "submit_job requires 'gpu_memory_limit_mb' when 'shared' is true"
    );
}

#[test]
fn preview_submit_jobs_expands_without_assigning_job_ids() {
    let output = preview_submit_jobs_output(
        vec![SubmitJobRequest {
            command: Some("echo {lr}".to_string()),
            script: None,
            gpus: Some(1),
            conda_env: None,
            run_dir: None,
            priority: None,
            depends_on: None,
            depends_on_ids: None,
            dependency_mode: None,
            auto_cancel_on_dependency_failure: None,
            shared: None,
            gpu_memory_limit_mb: None,
            time_limit_secs: None,
            memory_limit_mb: None,
            submitted_by: Some("alice".to_string()),
            param: Some(vec!["lr=0.1,0.2".to_string()]),
            parameters: None,
            run_name: None,
            project: None,
            max_concurrent: None,
            max_retries: None,
            auto_close_tmux: None,
            notify_email: None,
            notify_on: None,
        }],
        1,
    );

    assert!(output.dry_run);
    assert!(output.valid);
    assert_eq!(output.input_count, 1);
    assert_eq!(output.expanded_count, 2);
    assert_eq!(output.jobs.len(), 2);
    for result in output.jobs {
        assert!(result.ok);
        assert_eq!(result.input_index, 0);
        assert_eq!(result.job.unwrap()["id"], 0);
    }
}

#[test]
fn preview_update_job_reports_before_after_without_mutating_original() {
    let mut job = JobBuilder::new()
        .command("echo old")
        .submitted_by("alice")
        .gpus(1)
        .priority(10)
        .build();
    job.id = 7;
    job.state = JobState::Queued;

    let request = UpdateJobRequest {
        command: Some("echo new".to_string()),
        gpus: Some(2),
        priority: Some(5),
        memory_limit_mb: Some(Some(4096)),
        ..Default::default()
    };

    let output = preview_update_job_output(job, request);

    assert!(output.dry_run);
    assert!(output.ok);
    assert_eq!(output.job_id, 7);
    assert_eq!(
        output.updated_fields,
        vec!["command", "gpus", "priority", "memory_limit_mb"]
    );

    let before = output.before.expect("before should be present");
    let after = output.after.expect("after should be present");
    assert_eq!(before["command"], "echo old");
    assert_eq!(before["gpus"], 1);
    assert_eq!(before["priority"], 10);
    assert_eq!(after["command"], "echo new");
    assert_eq!(after["gpus"], 2);
    assert_eq!(after["priority"], 5);
    assert_eq!(after["memory_limit_mb"], 4096);
}

#[test]
fn triage_job_includes_log_based_retry_hints_and_exit_status_note() {
    let mut job = JobBuilder::new()
        .command("python train.py")
        .submitted_by("alice")
        .gpus(1)
        .max_retries(2)
        .build();
    job.id = 11;
    job.state = JobState::Failed;
    job.started_at = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(10));
    job.finished_at = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(25));

    let output = build_triage_job_output(
        job,
        Some("/tmp/gflow-11.log".to_string()),
        Some("RuntimeError: CUDA OOM while allocating tensor".to_string()),
    )
    .expect("triage output should build");

    assert_eq!(output.job_id, 11);
    assert_eq!(output.state, "Failed");
    assert_eq!(output.runtime_secs, Some(15.0));
    assert_eq!(output.exit_status, None);
    assert!(output
        .exit_status_note
        .contains("not the process exit code"));
    assert_eq!(output.log_path.as_deref(), Some("/tmp/gflow-11.log"));
    assert!(output
        .retry_hints
        .iter()
        .any(|hint| hint.contains("log suggests OOM")));
    assert!(output
        .retry_hints
        .iter()
        .any(|hint| hint.contains("max_retries=2")));
}

#[test]
fn queue_pressure_summarizes_gpu_pressure_and_groups() {
    let info = SchedulerInfo {
        gpus: vec![
            GpuInfo {
                uuid: "gpu-0".to_string(),
                index: 0,
                available: false,
                reason: Some("running gflow job".to_string()),
            },
            GpuInfo {
                uuid: "gpu-1".to_string(),
                index: 1,
                available: true,
                reason: None,
            },
        ],
        allowed_gpu_indices: None,
        gpu_allocation_strategy: GpuAllocationStrategy::Sequential,
    };

    let mut running = JobBuilder::new()
        .command("python train.py")
        .submitted_by("alice")
        .project(Some("vision".to_string()))
        .gpus(1)
        .build();
    running.id = 1;
    running.state = JobState::Running;
    running.gpu_ids = Some(vec![0].into());

    let mut queued = JobBuilder::new()
        .command("python eval.py")
        .submitted_by("alice")
        .project(Some("vision".to_string()))
        .gpus(2)
        .build();
    queued.id = 2;
    queued.state = JobState::Queued;
    queued.reason = Some(Box::new(JobStateReason::WaitingForGpu));

    let mut held = JobBuilder::new()
        .command("echo held")
        .submitted_by("bob")
        .gpus(0)
        .build();
    held.id = 3;
    held.state = JobState::Hold;

    let reservation = GpuReservation {
        id: 1,
        user: CompactString::from("alice"),
        gpu_spec: GpuSpec::Count(1),
        start_time: SystemTime::now() - Duration::from_secs(60),
        duration: Duration::from_secs(3600),
        status: ReservationStatus::Active,
        created_at: SystemTime::now() - Duration::from_secs(120),
        cancelled_at: None,
    };

    let output = build_queue_pressure_output(info, vec![running, queued, held], vec![reservation]);

    assert_eq!(output.total_gpus, 2);
    assert_eq!(output.available_gpus, vec![1]);
    assert_eq!(output.unavailable_gpus.len(), 1);
    assert_eq!(output.running_jobs, 1);
    assert_eq!(output.queued_jobs, 1);
    assert_eq!(output.held_jobs, 1);
    assert_eq!(output.queued_requested_gpus, 2);
    assert_eq!(output.running_allocated_gpus, 1);
    assert_eq!(output.blocked_reasons.get("Resources"), Some(&1));
    assert_eq!(output.blocked_reasons.get("JobHeldUser"), Some(&1));
    assert_eq!(output.reservations_total, 1);
    assert_eq!(output.reservations_active, 1);
    assert_eq!(output.users[0].name, "alice");
    assert_eq!(output.users[0].queued, 1);
    assert_eq!(output.users[0].running, 1);
    assert_eq!(output.projects[0].name, "vision");
}

#[test]
fn submit_job_maps_notifications_from_mcp_fields() {
    let job = build_submit_job(SubmitJobRequest {
        command: Some("echo hello".to_string()),
        script: None,
        gpus: Some(0),
        conda_env: None,
        run_dir: None,
        priority: None,
        depends_on: None,
        depends_on_ids: None,
        dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        shared: None,
        gpu_memory_limit_mb: None,
        time_limit_secs: None,
        memory_limit_mb: None,
        submitted_by: None,
        param: None,
        parameters: None,
        run_name: None,
        project: None,
        max_concurrent: None,
        max_retries: None,
        auto_close_tmux: None,
        notify_email: Some(vec!["alice@example.com".to_string()]),
        notify_on: Some(vec!["JOB_FAILED".to_string(), "job_timeout".to_string()]),
    })
    .expect("submit job should build");

    assert_eq!(job.notifications.emails.len(), 1);
    assert_eq!(job.notifications.emails[0].as_str(), "alice@example.com");
    assert_eq!(
        job.notifications
            .events
            .iter()
            .map(|event| event.as_str())
            .collect::<Vec<_>>(),
        vec!["job_failed", "job_timeout"]
    );
}

#[test]
fn update_job_maps_notifications_from_mcp_fields() {
    let request = build_update_request(UpdateJobToolRequest {
        job_id: 7,
        command: None,
        script: None,
        gpus: None,
        conda_env: None,
        clear_conda_env: None,
        priority: None,
        parameters: None,
        time_limit_secs: None,
        clear_time_limit: None,
        memory_limit_mb: None,
        clear_memory_limit: None,
        gpu_memory_limit_mb: None,
        clear_gpu_memory_limit: None,
        depends_on_ids: None,
        dependency_mode: None,
        clear_dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        max_concurrent: None,
        clear_max_concurrent: None,
        max_retries: None,
        clear_max_retries: None,
        notify_email: Some(vec!["alice@example.com".to_string()]),
        notify_on: Some(vec!["job_failed".to_string()]),
    })
    .expect("update request should build");

    let notifications = request
        .notifications
        .expect("notifications should be present");
    assert_eq!(notifications.emails.len(), 1);
    assert_eq!(notifications.emails[0].as_str(), "alice@example.com");
    assert_eq!(
        notifications
            .events
            .iter()
            .map(|event| event.as_str())
            .collect::<Vec<_>>(),
        vec!["job_failed"]
    );
}

#[test]
fn update_job_rejects_notify_on_without_notify_email() {
    let err = build_update_request(UpdateJobToolRequest {
        job_id: 7,
        command: None,
        script: None,
        gpus: None,
        conda_env: None,
        clear_conda_env: None,
        priority: None,
        parameters: None,
        time_limit_secs: None,
        clear_time_limit: None,
        memory_limit_mb: None,
        clear_memory_limit: None,
        gpu_memory_limit_mb: None,
        clear_gpu_memory_limit: None,
        depends_on_ids: None,
        dependency_mode: None,
        clear_dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        max_concurrent: None,
        clear_max_concurrent: None,
        max_retries: None,
        clear_max_retries: None,
        notify_email: None,
        notify_on: Some(vec!["job_failed".to_string()]),
    })
    .unwrap_err();

    assert_eq!(
        err,
        "update_job requires 'notify_email' when 'notify_on' is set"
    );
}

#[test]
fn list_outputs_expose_object_schemas() {
    let tools = GflowMcpServer::tool_router().list_all();

    for tool_name in ["list_jobs", "list_reservations"] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == tool_name)
            .unwrap_or_else(|| panic!("missing tool: {tool_name}"));
        assert!(
            tool.output_schema.is_some(),
            "expected output schema for {tool_name}"
        );
    }
}

#[test]
fn list_jobs_defaults_to_recent_first_paging() {
    let resolved = resolve_list_jobs_page(&ListJobsRequest {
        state: None,
        user: None,
        limit: None,
        offset: None,
        created_after: None,
        order: None,
        detail: None,
    });

    assert_eq!(resolved.limit, DEFAULT_MCP_LIST_JOBS_LIMIT);
    assert_eq!(resolved.offset, 0);
    assert_eq!(resolved.order, ListJobsOrderInput::Desc);
    assert_eq!(resolved.detail, ListJobsDetailInput::Summary);
    assert_eq!(resolved.query_limit, DEFAULT_MCP_LIST_JOBS_LIMIT + 1);
}

#[test]
fn list_jobs_honors_explicit_paging_request() {
    let resolved = resolve_list_jobs_page(&ListJobsRequest {
        state: Some("Running".to_string()),
        user: Some("alice".to_string()),
        limit: Some(12),
        offset: Some(24),
        created_after: Some(1_700_000_000),
        order: Some(ListJobsOrderInput::Asc),
        detail: Some(ListJobsDetailInput::Full),
    });

    assert_eq!(resolved.limit, 12);
    assert_eq!(resolved.offset, 24);
    assert_eq!(resolved.order, ListJobsOrderInput::Asc);
    assert_eq!(resolved.detail, ListJobsDetailInput::Full);
    assert_eq!(resolved.query_limit, 13);
}

#[test]
fn list_jobs_order_and_detail_deserialize_from_lowercase() {
    let request: ListJobsRequest = serde_json::from_str(
        r#"{"state": null, "user": null, "limit": null, "offset": null, "created_after": null, "order": "asc", "detail": "full"}"#,
    )
    .expect("lowercase order/detail should deserialize");

    assert_eq!(request.order, Some(ListJobsOrderInput::Asc));
    assert_eq!(request.detail, Some(ListJobsDetailInput::Full));
}

#[test]
fn list_jobs_output_schema_includes_pagination_fields() {
    let schema = schema_for!(ListJobsOutput);
    let schema_json = serde_json::to_value(&schema).expect("schema should serialize");
    let properties = schema_json
        .get("properties")
        .and_then(Value::as_object)
        .expect("schema should expose properties");

    for field in [
        "jobs",
        "count",
        "detail",
        "limit",
        "offset",
        "has_more",
        "next_offset",
    ] {
        assert!(
            properties.contains_key(field),
            "missing list_jobs output field in schema: {field}"
        );
    }
}

#[test]
fn get_job_log_supports_first_lines() {
    let slice = resolve_log_slice(&GetJobLogRequest {
        job_id: 7,
        first_lines: Some(10),
        last_lines: None,
        max_bytes: None,
    })
    .expect("first_lines should resolve");

    assert_eq!(slice, TextSlice::First(10));
}

#[test]
fn get_job_log_accepts_tail_lines_as_deprecated_alias() {
    let params: GetJobLogRequest = serde_json::from_value(serde_json::json!({
        "job_id": 7,
        "tail_lines": 25
    }))
    .expect("tail_lines alias should deserialize");
    let slice = resolve_log_slice(&params).expect("tail_lines alias should resolve");

    assert_eq!(slice, TextSlice::Last(25));
}

#[test]
fn get_job_log_rejects_conflicting_line_slice_options() {
    let err = resolve_log_slice(&GetJobLogRequest {
        job_id: 7,
        first_lines: Some(10),
        last_lines: Some(20),
        max_bytes: None,
    })
    .expect_err("conflicting options should fail");

    assert!(err
        .to_string()
        .contains("only one of first_lines or last_lines"));
}

#[test]
fn get_job_log_schema_hides_deprecated_tail_lines_field() {
    let schema = schema_for!(GetJobLogRequest);
    let schema_json = serde_json::to_value(&schema).expect("schema should serialize");
    let properties = schema_json
        .get("properties")
        .and_then(Value::as_object)
        .expect("schema should expose properties");

    assert!(properties.contains_key("first_lines"));
    assert!(properties.contains_key("last_lines"));
    assert!(!properties.contains_key("tail_lines"));
}

#[test]
fn slice_text_can_take_first_lines() {
    let output = slice_text("a\nb\nc\nd".to_string(), TextSlice::First(2), None);
    assert_eq!(output, "a\nb");
}

#[test]
fn slice_text_can_take_last_lines() {
    let output = slice_text("a\nb\nc\nd".to_string(), TextSlice::Last(2), None);
    assert_eq!(output, "c\nd");
}

#[test]
fn list_jobs_summary_is_compact() {
    let mut job = JobBuilder::new()
        .command("python train.py --epochs 100")
        .submitted_by("alice")
        .run_name(Some("exp-42".to_string()))
        .project(Some("vision".to_string()))
        .gpus(2)
        .build();
    job.id = 42;
    job.state = JobState::Running;
    job.reason = Some(Box::new(JobStateReason::WaitingForResources));

    let value = serialize_list_job(job, ListJobsDetailInput::Summary);
    let object = value.as_object().expect("summary should be an object");

    for field in [
        "id",
        "name",
        "state",
        "reason",
        "gpus",
        "gpu_ids",
        "user",
        "project",
        "submitted",
        "started",
        "finished",
    ] {
        assert!(object.contains_key(field), "missing summary field: {field}");
    }

    for field in [
        "command",
        "script",
        "conda_env",
        "run_dir",
        "parameters",
        "depends_on",
        "depends_on_ids",
        "memory_limit_mb",
        "time_limit",
    ] {
        assert!(
            !object.contains_key(field),
            "summary should omit verbose field: {field}"
        );
    }
}

#[test]
fn submit_jobs_expand_cli_style_param_combinations() {
    let expanded = expand_submit_job_requests(vec![SubmitJobRequest {
        command: Some("python train.py --lr {lr} --batch-size {bs}".to_string()),
        script: None,
        gpus: Some(0),
        conda_env: None,
        run_dir: None,
        priority: None,
        depends_on: None,
        depends_on_ids: None,
        dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        shared: None,
        gpu_memory_limit_mb: None,
        time_limit_secs: None,
        memory_limit_mb: None,
        submitted_by: None,
        param: Some(vec!["lr=0.001,0.01".to_string(), "bs=32,64".to_string()]),
        parameters: Some(HashMap::from([("seed".to_string(), "123".to_string())])),
        run_name: None,
        project: None,
        max_concurrent: None,
        max_retries: None,
        auto_close_tmux: None,
        notify_email: None,
        notify_on: None,
    }])
    .unwrap();

    assert_eq!(expanded.len(), 4);
    assert!(expanded.iter().all(|(index, _)| *index == 0));
    assert!(expanded.iter().all(|(_, job)| job.param.is_none()));
    assert_eq!(
        expanded[0]
            .1
            .parameters
            .as_ref()
            .and_then(|params| params.get("seed"))
            .map(String::as_str),
        Some("123")
    );
    assert_eq!(
        expanded[0]
            .1
            .parameters
            .as_ref()
            .and_then(|params| params.get("lr"))
            .map(String::as_str),
        Some("0.001")
    );
    assert_eq!(
        expanded[3]
            .1
            .parameters
            .as_ref()
            .and_then(|params| params.get("lr"))
            .map(String::as_str),
        Some("0.01")
    );
    assert_eq!(
        expanded[3]
            .1
            .parameters
            .as_ref()
            .and_then(|params| params.get("bs"))
            .map(String::as_str),
        Some("64")
    );
}

#[test]
fn submit_jobs_reject_duplicate_keys_between_parameters_and_param() {
    let err = expand_submit_job_requests(vec![SubmitJobRequest {
        command: Some("echo {lr}".to_string()),
        script: None,
        gpus: None,
        conda_env: None,
        run_dir: None,
        priority: None,
        depends_on: None,
        depends_on_ids: None,
        dependency_mode: None,
        auto_cancel_on_dependency_failure: None,
        shared: None,
        gpu_memory_limit_mb: None,
        time_limit_secs: None,
        memory_limit_mb: None,
        submitted_by: None,
        param: Some(vec!["lr=0.001,0.01".to_string()]),
        parameters: Some(HashMap::from([("lr".to_string(), "0.1".to_string())])),
        run_name: None,
        project: None,
        max_concurrent: None,
        max_retries: None,
        auto_close_tmux: None,
        notify_email: None,
        notify_on: None,
    }])
    .unwrap_err();

    assert_eq!(
        err,
        "submit_job cannot use the same key in both 'parameters' and 'param': lr"
    );
}
