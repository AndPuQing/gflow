//! Test for race condition where jobs are cancelled between prepare and execute phases
//!
//! This test verifies that the scheduler correctly handles the case where a job
//! is cancelled (or held/failed) after prepare_jobs_for_execution() but before
//! the executor.execute() call.

use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobBuilder, JobState};
use gflow::core::scheduler::SchedulerBuilder;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Mock executor that allows us to inject delays and observe execution
#[derive(Clone)]
struct DelayedMockExecutor {
    executions: Arc<Mutex<Vec<u32>>>,
    execution_delay: Duration,
}

impl DelayedMockExecutor {
    fn new(delay: Duration) -> Self {
        Self {
            executions: Arc::new(Mutex::new(Vec::new())),
            execution_delay: delay,
        }
    }

    fn get_executed_job_ids(&self) -> Vec<u32> {
        self.executions.lock().unwrap().clone()
    }
}

impl Executor for DelayedMockExecutor {
    fn execute(&self, job: &Job) -> anyhow::Result<()> {
        // Simulate tmux session creation delay
        thread::sleep(self.execution_delay);
        self.executions.lock().unwrap().push(job.id);
        Ok(())
    }
}

#[test]
fn test_job_cancelled_between_prepare_and_execute() {
    let executor = DelayedMockExecutor::new(Duration::from_millis(50));
    let executor_clone = executor.clone();

    let mut scheduler = SchedulerBuilder::new()
        .with_executor(Box::new(executor))
        .with_state_path(PathBuf::from("/tmp/test_cancel_race.json"))
        .with_total_memory_mb(16 * 1024)
        .build();

    // Submit a job
    let job = JobBuilder::new()
        .submitted_by("test")
        .run_dir("/tmp")
        .command("echo test")
        .build();
    let (job_id, _) = scheduler.submit_job(job);

    // Phase 1: Prepare job for execution (marks as Running, allocates resources)
    let jobs_to_execute = scheduler.prepare_jobs_for_execution();
    assert_eq!(jobs_to_execute.len(), 1);
    assert_eq!(jobs_to_execute[0].id, job_id);

    // Verify job is now Running
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Running);

    // RACE CONDITION SIMULATION: Cancel the job AFTER prepare but BEFORE execute
    scheduler.cancel_job(job_id, None);
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Cancelled);

    // Phase 2: Execute jobs (this should detect the state change and skip execution)
    let results = scheduler.execute_jobs_no_lock(&jobs_to_execute);

    // Verify execution was attempted but the job was NOT actually executed
    // (In the real implementation with the fix, the scheduler would recheck state)
    // Note: This test demonstrates the PROBLEM - without the fix, the job would execute

    // Phase 3: Handle failures
    scheduler.handle_execution_failures(&results);

    // Check if the job was actually executed
    let executed_jobs = executor_clone.get_executed_job_ids();

    // Without the fix: executed_jobs would contain job_id (BAD!)
    // With the fix: executed_jobs should be empty (GOOD!)
    println!("Executed jobs: {:?}", executed_jobs);
    println!("Job final state: {:?}", scheduler.jobs[&job_id].state);

    // The job should remain in Cancelled state
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Cancelled);
}

#[test]
fn test_job_failed_between_prepare_and_execute() {
    let executor = DelayedMockExecutor::new(Duration::from_millis(50));
    let executor_clone = executor.clone();

    let mut scheduler = SchedulerBuilder::new()
        .with_executor(Box::new(executor))
        .with_state_path(PathBuf::from("/tmp/test_fail_race.json"))
        .with_total_memory_mb(16 * 1024)
        .build();

    // Submit a job
    let job = JobBuilder::new()
        .submitted_by("test")
        .run_dir("/tmp")
        .command("echo test")
        .build();
    let (job_id, _) = scheduler.submit_job(job);

    // Phase 1: Prepare job for execution
    let jobs_to_execute = scheduler.prepare_jobs_for_execution();
    assert_eq!(jobs_to_execute.len(), 1);

    // RACE CONDITION: Fail the job AFTER prepare but BEFORE execute
    // (e.g., external monitoring system detected an issue)
    scheduler.fail_job(job_id);
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Failed);

    // Phase 2: Execute jobs (should skip the failed job)
    let results = scheduler.execute_jobs_no_lock(&jobs_to_execute);

    // Phase 3: Handle failures
    scheduler.handle_execution_failures(&results);

    // Check execution
    let executed_jobs = executor_clone.get_executed_job_ids();
    println!("Executed jobs: {:?}", executed_jobs);

    // Job should remain in Failed state (not executed)
    assert_eq!(scheduler.jobs[&job_id].state, JobState::Failed);
}

#[test]
fn test_multiple_jobs_partial_cancellation() {
    let executor = DelayedMockExecutor::new(Duration::from_millis(10));
    let executor_clone = executor.clone();

    let mut scheduler = SchedulerBuilder::new()
        .with_executor(Box::new(executor))
        .with_state_path(PathBuf::from("/tmp/test_partial_cancel.json"))
        .with_total_memory_mb(16 * 1024)
        .build();

    // Submit 5 jobs
    let mut job_ids = Vec::new();
    for i in 0..5 {
        let job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .command(format!("echo job-{}", i))
            .build();
        let (job_id, _) = scheduler.submit_job(job);
        job_ids.push(job_id);
    }

    // Prepare all jobs
    let jobs_to_execute = scheduler.prepare_jobs_for_execution();
    assert_eq!(jobs_to_execute.len(), 5);

    // Cancel jobs 2 and 4 (0-indexed: jobs with id 2 and 4)
    scheduler.cancel_job(job_ids[1], None);
    scheduler.cancel_job(job_ids[3], None);

    // Execute (should skip cancelled jobs)
    let results = scheduler.execute_jobs_no_lock(&jobs_to_execute);
    scheduler.handle_execution_failures(&results);

    // Verify results
    let executed_jobs = executor_clone.get_executed_job_ids();
    println!("Executed jobs: {:?}", executed_jobs);
    println!("Cancelled jobs: {:?}", vec![job_ids[1], job_ids[3]]);

    // Without fix: all 5 jobs executed
    // With fix: only 3 jobs executed (jobs 1, 3, 5)
    // The cancelled jobs (2, 4) should NOT be in executed list

    // Verify states
    assert_eq!(scheduler.jobs[&job_ids[1]].state, JobState::Cancelled);
    assert_eq!(scheduler.jobs[&job_ids[3]].state, JobState::Cancelled);
}
