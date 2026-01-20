//! Integration tests for the gflow job scheduler
//!
//! These tests verify end-to-end functionality using a test Scheduler instance
//! with a MockExecutor and temporary state file.

#![allow(deprecated)]

use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobBuilder, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::core::GPUSlot;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Mock executor for testing that records executed jobs
#[derive(Clone)]
struct MockExecutor {
    executions: Arc<Mutex<Vec<Job>>>,
    should_fail: bool,
}

impl MockExecutor {
    fn new() -> Self {
        Self {
            executions: Arc::new(Mutex::new(Vec::new())),
            should_fail: false,
        }
    }

    #[allow(dead_code)]
    fn with_failure(should_fail: bool) -> Self {
        Self {
            executions: Arc::new(Mutex::new(Vec::new())),
            should_fail,
        }
    }

    fn get_executions(&self) -> Vec<Job> {
        self.executions.lock().unwrap().clone()
    }

    fn execution_count(&self) -> usize {
        self.executions.lock().unwrap().len()
    }

    fn clear(&self) {
        self.executions.lock().unwrap().clear();
    }
}

impl Executor for MockExecutor {
    fn execute(&self, job: &Job) -> anyhow::Result<()> {
        if self.should_fail {
            anyhow::bail!("Mock execution failed")
        } else {
            self.executions.lock().unwrap().push(job.clone());
            Ok(())
        }
    }
}

/// Helper function to create a test scheduler with mock executor
fn create_test_scheduler() -> (Scheduler, MockExecutor) {
    let executor = MockExecutor::new();
    let executor_clone = executor.clone();

    // Create GPU slots
    let mut gpu_slots = HashMap::new();
    gpu_slots.insert(
        "GPU-0".to_string(),
        GPUSlot {
            index: 0,
            available: true,
        },
    );
    gpu_slots.insert(
        "GPU-1".to_string(),
        GPUSlot {
            index: 1,
            available: true,
        },
    );

    let scheduler = SchedulerBuilder::new()
        .with_executor(Box::new(executor))
        .with_state_path(PathBuf::from("/tmp/test_scheduler.json"))
        .with_total_memory_mb(8192) // 8GB
        .with_gpu_slots(gpu_slots)
        .build();

    (scheduler, executor_clone)
}

/// Helper function to create a basic test job
fn create_test_job(username: &str) -> Job {
    JobBuilder::new()
        .submitted_by(username.to_string())
        .run_dir("/tmp")
        .command("echo test")
        .build()
}

// ============================================================================
// Job Submission and Queuing Tests
// ============================================================================

#[test]
fn test_job_submission_and_queuing() {
    let (mut scheduler, _) = create_test_scheduler();

    // Submit first job
    let job1 = create_test_job("alice");
    let (job_id1, run_name1) = scheduler.submit_job(job1);
    assert_eq!(job_id1, 1);
    assert_eq!(run_name1, "gflow-job-1");
    assert!(scheduler.jobs.contains_key(&1));
    assert_eq!(scheduler.jobs[&1].state, JobState::Queued);

    // Submit second job
    let job2 = create_test_job("bob");
    let (job_id2, run_name2) = scheduler.submit_job(job2);
    assert_eq!(job_id2, 2);
    assert_eq!(run_name2, "gflow-job-2");
    assert!(scheduler.jobs.contains_key(&2));
    assert_eq!(scheduler.jobs[&2].state, JobState::Queued);

    // Verify queue state
    assert_eq!(scheduler.jobs.len(), 2);
}

#[test]
fn test_job_execution_from_queue() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit a job
    let job = create_test_job("alice");
    let (job_id, _) = scheduler.submit_job(job);

    // Schedule jobs - should execute the job
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_ok());

    // Verify job is running
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Running);

    // Verify executor was called
    assert_eq!(executor.execution_count(), 1);
    let executed_jobs = executor.get_executions();
    assert_eq!(executed_jobs[0].id, job_id);
}

#[test]
fn test_multiple_jobs_execution() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit 3 jobs
    for i in 0..3 {
        let job = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .command(format!("echo job{}", i))
            .build();
        scheduler.submit_job(job);
    }

    // Schedule jobs - should execute all jobs (we have 2 GPUs, jobs don't require GPUs)
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 3);

    // All jobs should have been executed
    assert_eq!(executor.execution_count(), 3);

    // Verify all jobs are running
    for job_id in 1..=3 {
        assert_eq!(scheduler.jobs[&job_id].state, JobState::Running);
    }
}

// ============================================================================
// Dependency Resolution Tests
// ============================================================================

#[test]
fn test_dependency_resolution_basic() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit job A
    let job_a = create_test_job("alice");
    let (job_a_id, _) = scheduler.submit_job(job_a);

    // Submit job B that depends on A
    let job_b = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .command("echo job_b")
        .depends_on(Some(job_a_id))
        .build();
    let (job_b_id, _) = scheduler.submit_job(job_b);

    // First schedule - only job A should run
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, job_a_id);
    assert_eq!(scheduler.jobs[&job_a_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Queued);

    // Try scheduling again - job B should still be waiting
    executor.clear();
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 0);
    assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Queued);

    // Finish job A
    scheduler.finish_job(job_a_id);
    assert_eq!(scheduler.jobs[&job_a_id].state, JobState::Finished);

    // Now job B should run
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, job_b_id);
    assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Running);
}

#[test]
fn test_dependency_chain() {
    let (mut scheduler, _executor) = create_test_scheduler();

    // Create a chain: A -> B -> C
    let job_a = create_test_job("alice");
    let (job_a_id, _) = scheduler.submit_job(job_a);

    let job_b = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .depends_on(Some(job_a_id))
        .build();
    let (job_b_id, _) = scheduler.submit_job(job_b);

    let job_c = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .depends_on(Some(job_b_id))
        .build();
    let (job_c_id, _) = scheduler.submit_job(job_c);

    // First schedule - only A runs
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job_a_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Queued);
    assert_eq!(scheduler.jobs[&job_c_id].state, JobState::Queued);

    // Finish A, schedule - B runs
    scheduler.finish_job(job_a_id);
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job_a_id].state, JobState::Finished);
    assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job_c_id].state, JobState::Queued);

    // Finish B, schedule - C runs
    scheduler.finish_job(job_b_id);
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Finished);
    assert_eq!(scheduler.jobs[&job_c_id].state, JobState::Running);
}

#[test]
fn test_dependency_not_started_if_parent_failed() {
    let (mut scheduler, _executor) = create_test_scheduler();

    // Submit job A
    let job_a = create_test_job("alice");
    let (job_a_id, _) = scheduler.submit_job(job_a);

    // Submit job B that depends on A
    let job_b = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .depends_on(Some(job_a_id))
        .build();
    let (job_b_id, _) = scheduler.submit_job(job_b);

    // Schedule and run job A
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job_a_id].state, JobState::Running);

    // Fail job A
    scheduler.fail_job(job_a_id);
    assert_eq!(scheduler.jobs[&job_a_id].state, JobState::Failed);

    // Job B should not run because A failed
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 0);
    assert_eq!(scheduler.jobs[&job_b_id].state, JobState::Queued);
}

// ============================================================================
// Priority Scheduling Tests
// ============================================================================

#[test]
fn test_priority_scheduling_high_priority_first() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit low priority job
    let low_priority_job = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .priority(5)
        .gpus(1) // Requires GPU so only one runs at a time
        .build();
    let (low_id, _) = scheduler.submit_job(low_priority_job);

    // Submit high priority job
    let high_priority_job = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .priority(20)
        .gpus(1) // Requires GPU
        .build();
    let (high_id, _) = scheduler.submit_job(high_priority_job);

    // Schedule - high priority should run first
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 2); // Both can run (2 GPUs available)

    let executions = executor.get_executions();
    // High priority should be executed first
    assert_eq!(executions[0].id, high_id);
    assert_eq!(executions[0].priority, 20);
    assert_eq!(executions[1].id, low_id);
    assert_eq!(executions[1].priority, 5);
}

#[test]
fn test_priority_with_time_bonus() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Job with same priority but shorter time limit gets higher effective priority
    let long_job = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .priority(10)
        .time_limit(Some(Duration::from_secs(3600 * 24))) // 24 hours
        .build();
    scheduler.submit_job(long_job);

    let short_job = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .priority(10)
        .time_limit(Some(Duration::from_secs(60))) // 1 minute
        .build();
    let (short_id, _) = scheduler.submit_job(short_job);

    // Schedule - short job should run first due to time bonus
    scheduler.schedule_jobs();

    let executions = executor.get_executions();
    // Short job should be executed first (time bonus gives it higher effective priority)
    assert_eq!(executions[0].id, short_id);
}

#[test]
fn test_priority_tiebreaker_with_job_id() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit 3 jobs with same priority
    for _ in 0..3 {
        let job = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .priority(10)
            .build();
        scheduler.submit_job(job);
    }

    // Schedule all jobs
    scheduler.schedule_jobs();

    let executions = executor.get_executions();
    // Should execute in order of submission (job ID)
    assert_eq!(executions[0].id, 1);
    assert_eq!(executions[1].id, 2);
    assert_eq!(executions[2].id, 3);
}

// ============================================================================
// Resource Constraint Tests
// ============================================================================

#[test]
fn test_gpu_constraints() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit 3 jobs, each requiring 1 GPU (we only have 2 GPUs)
    for _ in 0..3 {
        let job = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .build();
        scheduler.submit_job(job);
    }

    // First schedule - only 2 jobs should run
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 2);
    assert_eq!(executor.execution_count(), 2);

    // Jobs 1 and 2 should be running, job 3 should be queued
    assert_eq!(scheduler.jobs[&1].state, JobState::Running);
    assert_eq!(scheduler.jobs[&2].state, JobState::Running);
    assert_eq!(scheduler.jobs[&3].state, JobState::Queued);

    // Verify GPU allocation
    assert_eq!(scheduler.jobs[&1].gpu_ids, Some(vec![0]));
    assert_eq!(scheduler.jobs[&2].gpu_ids, Some(vec![1]));

    // Mark GPUs as unavailable (simulating what gflowd would do)
    scheduler
        .gpu_slots_mut()
        .get_mut("GPU-0")
        .unwrap()
        .available = false;
    scheduler
        .gpu_slots_mut()
        .get_mut("GPU-1")
        .unwrap()
        .available = false;

    // Second schedule - job 3 should still be waiting (no GPUs available)
    executor.clear();
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 0);
    assert_eq!(scheduler.jobs[&3].state, JobState::Queued);
}

#[test]
fn test_multi_gpu_job() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit a job requiring 2 GPUs
    let job = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .gpus(2)
        .build();
    let (job_id, _) = scheduler.submit_job(job);

    // Schedule - job should run and get both GPUs
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 1);
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job_id].gpu_ids, Some(vec![0, 1]));

    // Verify execution
    assert_eq!(executor.execution_count(), 1);
}

#[test]
fn test_insufficient_gpus() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit a job requiring 3 GPUs (we only have 2)
    let job = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .gpus(3)
        .build();
    let (job_id, _) = scheduler.submit_job(job);

    // Schedule - job should not run
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 0);
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Queued);
    assert_eq!(executor.execution_count(), 0);
}

#[test]
fn test_memory_constraints() {
    let (mut scheduler, _executor) = create_test_scheduler();

    // Submit a job requiring 4GB
    let job1 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .memory_limit_mb(Some(4096))
        .build();
    let (job1_id, _) = scheduler.submit_job(job1);

    // Submit a job requiring 3GB
    let job2 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .memory_limit_mb(Some(3072))
        .build();
    let (job2_id, _) = scheduler.submit_job(job2);

    // Submit a job requiring 2GB
    let job3 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .memory_limit_mb(Some(2048))
        .build();
    let (job3_id, _) = scheduler.submit_job(job3);

    // Schedule - only 2 jobs should start (4GB + 3GB = 7GB < 8GB)
    // The third job (2GB) would exceed the limit (7GB + 2GB = 9GB > 8GB)
    // Note: Fixed memory accounting now tracks available memory correctly
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 2);

    assert_eq!(scheduler.jobs[&job1_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job2_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job3_id].state, JobState::Queued); // Waiting for memory

    // Verify memory is tracked correctly
    scheduler.refresh_available_memory();
    // After 2 jobs running: 8192 - 4096 - 3072 = 1024 MB available

    // Finish job1 and refresh memory
    scheduler.finish_job(job1_id);
    scheduler.refresh_available_memory();
    // After job1 finishes: 8192 - 3072 = 5120 MB available

    // Now job3 should be able to start
    let results2 = scheduler.schedule_jobs();
    assert_eq!(results2.len(), 1);
    assert_eq!(scheduler.jobs[&job3_id].state, JobState::Running);
}

#[test]
fn test_job_exceeds_total_memory() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Submit a job requiring more memory than available
    let job = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .memory_limit_mb(Some(10240)) // 10GB, but we only have 8GB
        .build();
    let (job_id, _) = scheduler.submit_job(job);

    // Schedule - job should not run
    let results = scheduler.schedule_jobs();
    assert_eq!(results.len(), 0);
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Queued);
    assert_eq!(executor.execution_count(), 0);
}

// ============================================================================
// Combined Resource and Priority Tests
// ============================================================================

#[test]
fn test_priority_with_resource_constraints() {
    let (mut scheduler, executor) = create_test_scheduler();

    // Low priority job requiring 1 GPU
    let low_priority = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .priority(5)
        .gpus(1)
        .build();
    scheduler.submit_job(low_priority);

    // High priority job requiring 2 GPUs
    let high_priority = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .priority(20)
        .gpus(2)
        .build();
    let (high_id, _) = scheduler.submit_job(high_priority);

    // Schedule - high priority job should run first and get both GPUs
    let _results = scheduler.schedule_jobs();

    // High priority job executed first
    let executions = executor.get_executions();
    assert_eq!(executions[0].id, high_id);
    assert_eq!(executions[0].gpu_ids, Some(vec![0, 1]));
}

#[test]
fn test_group_concurrency_limit() {
    let (mut scheduler, _executor) = create_test_scheduler();

    let group_id = "test-group".to_string();

    // Submit 3 jobs in the same group with max_concurrent = 2
    for _ in 0..3 {
        let job = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .group_id(Some(group_id.clone()))
            .max_concurrent(Some(2))
            .build();
        scheduler.submit_job(job);
    }

    // Schedule - only 2 jobs should run (group limit)
    scheduler.schedule_jobs();

    assert_eq!(scheduler.jobs[&1].state, JobState::Running);
    assert_eq!(scheduler.jobs[&2].state, JobState::Running);
    assert_eq!(scheduler.jobs[&3].state, JobState::Queued);

    // Finish one job
    scheduler.finish_job(1);

    // Now the third job should run
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&3].state, JobState::Running);
}

// ============================================================================
// Cascade Redo Tests
// ============================================================================

#[test]
fn test_cascade_redo_dependency_chain() {
    use gflow::core::job::JobStateReason;

    let (mut scheduler, _executor) = create_test_scheduler();

    // Create a dependency chain: Job 1 -> Job 2 -> Job 3
    // Job 1 (parent)
    let job1 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .command("echo job1")
        .build();
    let (job1_id, _) = scheduler.submit_job(job1);

    // Job 2 (depends on Job 1)
    let job2 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .command("echo job2")
        .depends_on_ids(vec![job1_id])
        .auto_cancel_on_dependency_failure(true)
        .build();
    let (job2_id, _) = scheduler.submit_job(job2);

    // Job 3 (depends on Job 2)
    let job3 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .command("echo job3")
        .depends_on_ids(vec![job2_id])
        .auto_cancel_on_dependency_failure(true)
        .build();
    let (job3_id, _) = scheduler.submit_job(job3);

    // Schedule and run Job 1
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job1_id].state, JobState::Running);

    // Fail Job 1 - this should cascade cancel Job 2 and Job 3
    scheduler.fail_job(job1_id);
    assert_eq!(scheduler.jobs[&job1_id].state, JobState::Failed);

    // Trigger auto-cancellation
    scheduler.auto_cancel_dependent_jobs(job1_id);

    // Verify cascade cancellation
    assert_eq!(scheduler.jobs[&job2_id].state, JobState::Cancelled);
    assert_eq!(
        scheduler.jobs[&job2_id].reason,
        Some(JobStateReason::DependencyFailed(job1_id))
    );
    assert_eq!(scheduler.jobs[&job3_id].state, JobState::Cancelled);
    assert_eq!(
        scheduler.jobs[&job3_id].reason,
        Some(JobStateReason::DependencyFailed(job2_id))
    );

    // Now simulate cascade redo:
    // 1. Redo Job 1 (creates Job 4)
    let job4 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .command("echo job1")
        .redone_from(Some(job1_id))
        .build();
    let (job4_id, _) = scheduler.submit_job(job4);

    // 2. Redo Job 2 with updated dependency (creates Job 5)
    let job5 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .command("echo job2")
        .depends_on_ids(vec![job4_id]) // Updated to depend on Job 4
        .auto_cancel_on_dependency_failure(true)
        .redone_from(Some(job2_id))
        .build();
    let (job5_id, _) = scheduler.submit_job(job5);

    // 3. Redo Job 3 with updated dependency (creates Job 6)
    let job6 = JobBuilder::new()
        .submitted_by("alice")
        .run_dir("/tmp")
        .command("echo job3")
        .depends_on_ids(vec![job5_id]) // Updated to depend on Job 5
        .auto_cancel_on_dependency_failure(true)
        .redone_from(Some(job3_id))
        .build();
    let (job6_id, _) = scheduler.submit_job(job6);

    // Verify the new jobs are queued
    assert_eq!(scheduler.jobs[&job4_id].state, JobState::Queued);
    assert_eq!(scheduler.jobs[&job5_id].state, JobState::Queued);
    assert_eq!(scheduler.jobs[&job6_id].state, JobState::Queued);

    // Schedule and run Job 4
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job4_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job5_id].state, JobState::Queued); // Still waiting

    // Finish Job 4 - Job 5 should now run
    scheduler.finish_job(job4_id);
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job4_id].state, JobState::Finished);
    assert_eq!(scheduler.jobs[&job5_id].state, JobState::Running);
    assert_eq!(scheduler.jobs[&job6_id].state, JobState::Queued); // Still waiting

    // Finish Job 5 - Job 6 should now run
    scheduler.finish_job(job5_id);
    scheduler.schedule_jobs();
    assert_eq!(scheduler.jobs[&job5_id].state, JobState::Finished);
    assert_eq!(scheduler.jobs[&job6_id].state, JobState::Running);

    // Finish Job 6
    scheduler.finish_job(job6_id);
    assert_eq!(scheduler.jobs[&job6_id].state, JobState::Finished);

    // Verify the cascade redo preserved the dependency chain
    assert_eq!(scheduler.jobs[&job5_id].depends_on_ids, vec![job4_id]);
    assert_eq!(scheduler.jobs[&job6_id].depends_on_ids, vec![job5_id]);
}
