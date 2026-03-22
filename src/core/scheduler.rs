use crate::core::executor::Executor;
use crate::core::gpu::{GPUSlot, GpuUuid};
use crate::core::gpu_allocation::GpuAllocationStrategy;
use crate::core::info::{GpuInfo, SchedulerInfo};
use crate::core::job::{
    DependencyMode, GpuIds, GpuSharingMode, Job, JobRuntime, JobSpec, JobState, JobStateReason,
    JobView,
};
use crate::core::reservation::{GpuReservation, ReservationStatus};
use compact_str::{format_compact, CompactString};
use serde::{Deserialize, Deserializer, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

#[path = "scheduler/access.rs"]
mod access;
#[path = "scheduler/builder.rs"]
mod builder;
#[path = "scheduler/persistence.rs"]
mod persistence;
#[path = "scheduler/reservations.rs"]
mod reservations;
#[path = "scheduler/scheduling.rs"]
mod scheduling;
#[path = "scheduler/transitions.rs"]
mod transitions;

pub use builder::SchedulerBuilder;

#[derive(Debug, Clone, Default)]
pub(crate) struct DependencyRuntime {
    pub total: u32,
    pub success: u32,
    pub terminal_non_success: u32,
    pub deps_satisfied: bool,
    pub impossible: bool,
    pub ready_epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadyEntry {
    pub job_id: u32,
    pub epoch: u64,
    pub priority: u8,
    pub time_bonus: u32,
}

impl Ord for ReadyEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.priority,
            self.time_bonus,
            std::cmp::Reverse(self.job_id),
            self.epoch,
        )
            .cmp(&(
                other.priority,
                other.time_bonus,
                std::cmp::Reverse(other.job_id),
                other.epoch,
            ))
    }
}

impl PartialOrd for ReadyEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Core scheduler with dependency injection for execution strategy
#[derive(Serialize)]
#[serde(default)]
pub struct Scheduler {
    #[serde(default)]
    pub version: u32,

    // Parallel vectors for split storage (serialized in v4+)
    #[serde(default)]
    pub(crate) job_specs: Vec<JobSpec>,
    #[serde(default)]
    pub(crate) job_runtimes: Vec<JobRuntime>,

    #[serde(skip)]
    pub(crate) executor: Option<Box<dyn Executor>>,
    #[serde(skip)]
    pub(crate) gpu_slots: HashMap<GpuUuid, GPUSlot>,
    #[serde(skip)]
    pub(crate) total_memory_mb: u64,
    #[serde(skip)]
    pub(crate) available_memory_mb: u64,
    pub(crate) state_path: PathBuf,
    pub(crate) next_job_id: u32,
    /// GPU indices that scheduler is allowed to use (None = all GPUs)
    pub(crate) allowed_gpu_indices: Option<Vec<u32>>,
    /// Strategy for selecting which available GPU indices to assign to a job.
    #[serde(skip)]
    pub(crate) gpu_allocation_strategy: GpuAllocationStrategy,
    /// Index of job IDs by username for fast dependency resolution
    /// Maps username -> sorted list of job IDs (ascending order)
    #[serde(skip)]
    pub(crate) user_jobs_index: HashMap<CompactString, Vec<u32>>,
    /// Index of job IDs by state for faster state filtering.
    /// Maps state -> sorted list of job IDs (ascending order)
    #[serde(skip)]
    pub(crate) state_jobs_index: HashMap<JobState, Vec<u32>>,
    /// Index of job IDs by project for fast project filtering.
    /// Maps project -> sorted list of job IDs (ascending order)
    #[serde(skip)]
    pub(crate) project_jobs_index: HashMap<CompactString, Vec<u32>>,
    /// Reverse dependency graph for fast dependent lookup
    /// Maps dependency job ID -> sorted list of dependent job IDs
    #[serde(skip)]
    pub(crate) dependents_graph: HashMap<u32, Vec<u32>>,
    /// Runtime dependency state aligned with job IDs (job_id - 1).
    #[serde(skip)]
    pub(crate) dependency_runtimes: Vec<DependencyRuntime>,
    /// Heap of queued jobs whose dependencies are already satisfied.
    #[serde(skip)]
    pub(crate) ready_heap: BinaryHeap<ReadyEntry>,
    /// Index of running job counts by group_id for O(1) group concurrency checks
    /// Maps group_id -> count of running jobs in that group
    #[serde(skip)]
    pub(crate) group_running_count: HashMap<uuid::Uuid, usize>,
    /// GPU reservations
    pub reservations: Vec<GpuReservation>,
    /// Next reservation ID
    pub next_reservation_id: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::job::JobBuilder;
    use serde::Serialize;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    /// Mock executor for testing
    struct MockExecutor {
        executions: Arc<Mutex<Vec<Job>>>,
        should_fail: bool,
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

    fn create_test_scheduler() -> Scheduler {
        let executor = Box::new(MockExecutor {
            executions: Arc::new(Mutex::new(Vec::new())),
            should_fail: false,
        });

        SchedulerBuilder::new()
            .with_executor(executor)
            .with_state_path(PathBuf::from("/tmp/test.json"))
            .with_total_memory_mb(16 * 1024)
            .build()
    }

    #[test]
    fn test_deserialize_legacy_jobs_map_msgpack_int_keys() {
        #[derive(Serialize)]
        struct LegacySchedulerState {
            version: u32,
            jobs: HashMap<u32, Job>,
            state_path: PathBuf,
            next_job_id: u32,
            allowed_gpu_indices: Option<Vec<u32>>,
            reservations: Vec<GpuReservation>,
            next_reservation_id: u32,
        }

        let mut jobs = HashMap::new();
        let mut job = JobBuilder::new().command("echo hi").gpus(1).build();
        job.id = 1;
        jobs.insert(1, job);

        let legacy = LegacySchedulerState {
            version: crate::core::migrations::CURRENT_VERSION,
            jobs,
            state_path: PathBuf::from("state.json"),
            next_job_id: 2,
            allowed_gpu_indices: None,
            reservations: Vec::new(),
            next_reservation_id: 1,
        };

        let bytes = rmp_serde::to_vec_named(&legacy).unwrap();
        let scheduler: Scheduler = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(scheduler.job_specs.len(), 1);
        assert_eq!(scheduler.job_runtimes.len(), 1);
        let cmd = scheduler
            .get_job_spec(1)
            .unwrap()
            .command
            .as_ref()
            .map(|s| s.as_str());
        assert_eq!(cmd, Some("echo hi"));
    }

    #[test]
    fn test_deserialize_legacy_scheduler_seq_msgpack_v2() {
        // Old state.msgpack layout (array of 5):
        // (version, jobs, state_path, next_job_id, allowed_gpu_indices)
        let mut job = JobBuilder::new().command("echo hi").gpus(1).build();
        job.id = 1;
        let jobs = vec![job];

        let legacy = (
            2u32,
            jobs,
            PathBuf::from("/home/happy/.local/share/gflow/state.json"),
            2u32,
            Option::<Vec<u32>>::None,
        );

        // Default rmp-serde encoding (array) matches what old gflowd wrote.
        let bytes = rmp_serde::to_vec(&legacy).unwrap();
        let scheduler: Scheduler = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(scheduler.job_specs.len(), 1);
        assert_eq!(scheduler.job_runtimes.len(), 1);
        let cmd = scheduler
            .get_job_spec(1)
            .unwrap()
            .command
            .as_ref()
            .map(|s| s.as_str());
        assert_eq!(cmd, Some("echo hi"));
    }

    fn create_test_job(username: &str) -> Job {
        JobBuilder::new()
            .submitted_by(username.to_string())
            .run_dir("/tmp")
            .build()
    }

    #[test]
    fn test_submit_job() {
        let mut scheduler = create_test_scheduler();
        let job = create_test_job("test");

        let (job_id, run_name) = scheduler.submit_job(job);
        assert_eq!(job_id, 1);
        assert_eq!(run_name, "gjob-1");
        assert!(scheduler.job_exists(1));
        assert_eq!(scheduler.get_job(1).unwrap().state, JobState::Queued);
    }

    #[test]
    fn test_submit_job_sets_waiting_for_dependency_reason() {
        let mut scheduler = create_test_scheduler();

        let parent = create_test_job("test");
        let (parent_id, _) = scheduler.submit_job(parent);

        let child = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![parent_id])
            .build();
        let (child_id, _) = scheduler.submit_job(child);

        assert_eq!(
            scheduler
                .get_job(child_id)
                .and_then(|j| j.reason.map(|r| *r)),
            Some(JobStateReason::WaitingForDependency)
        );
    }
    #[test]
    fn test_dependency_update_refresh_uses_wavefront_for_deep_queued_chain() {
        let mut scheduler = create_test_scheduler();

        let job_a = create_test_job("test");
        let (job_a_id, _) = scheduler.submit_job(job_a);

        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id])
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        let initial_epoch_c = scheduler
            .dependency_runtime(job_c_id)
            .map(|dep_rt| dep_rt.ready_epoch)
            .unwrap();

        let job_b_idx = (job_b_id - 1) as usize;
        scheduler.job_specs[job_b_idx].depends_on = None;
        scheduler.job_specs[job_b_idx].depends_on_ids.clear();
        scheduler.replace_job_dependencies(job_b_id, vec![job_a_id], vec![]);

        assert_eq!(
            scheduler
                .get_job(job_b_id)
                .and_then(|j| j.reason.map(|r| *r)),
            None
        );
        assert_eq!(
            scheduler
                .dependency_runtime(job_c_id)
                .map(|dep_rt| dep_rt.ready_epoch)
                .unwrap(),
            initial_epoch_c + 1
        );
        assert_eq!(
            scheduler
                .get_job(job_c_id)
                .and_then(|j| j.reason.map(|r| *r)),
            Some(JobStateReason::WaitingForDependency)
        );
    }

    #[test]
    fn test_terminal_dependency_propagation_keeps_ready_entry_valid() {
        let mut scheduler = create_test_scheduler();

        let parent = create_test_job("test");
        let (parent_id, _) = scheduler.submit_job(parent);

        let child = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![parent_id])
            .build();
        let (child_id, _) = scheduler.submit_job(child);

        scheduler.transition_job_state(parent_id, JobState::Running, None);
        scheduler.finish_job(parent_id);

        let prepared = scheduler.prepare_jobs_for_execution();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].id, child_id);
        assert_eq!(
            scheduler.get_job(child_id).map(|j| j.state),
            Some(JobState::Running)
        );
    }

    #[test]
    fn test_wavefront_refresh_requeues_already_ready_any_mode_job() {
        let mut scheduler = create_test_scheduler();

        let job_b_parent = create_test_job("test");
        let (job_b_parent_id, _) = scheduler.submit_job(job_b_parent);

        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_parent_id])
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        let job_x = create_test_job("test");
        let (job_x_id, _) = scheduler.submit_job(job_x);
        scheduler.transition_job_state(job_x_id, JobState::Running, None);
        scheduler.finish_job(job_x_id);

        let job_d = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id, job_x_id])
            .dependency_mode(Some(DependencyMode::Any))
            .build();
        let (job_d_id, _) = scheduler.submit_job(job_d);

        assert_eq!(
            scheduler
                .dependency_runtime(job_d_id)
                .map(|dep_rt| dep_rt.deps_satisfied),
            Some(true)
        );

        let job_b_idx = (job_b_id - 1) as usize;
        scheduler.job_specs[job_b_idx].depends_on = None;
        scheduler.job_specs[job_b_idx].depends_on_ids.clear();
        scheduler.replace_job_dependencies(job_b_id, vec![job_b_parent_id], vec![]);

        let prepared = scheduler.prepare_jobs_for_execution();
        assert!(prepared.iter().any(|job| job.id == job_d_id));
    }

    #[test]
    fn test_gpu_allocation_strategy_sequential_uses_lowest_indices_first() {
        let mut scheduler = create_test_scheduler();
        scheduler.set_gpu_allocation_strategy(GpuAllocationStrategy::Sequential);

        for i in 0..4 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    total_memory_mb: None,
                    reason: None,
                },
            );
        }

        let job = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(2)
            .build();
        let (job_id, _) = scheduler.submit_job(job);

        let prepared = scheduler.prepare_jobs_for_execution();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].id, job_id);
        assert_eq!(
            scheduler.get_job(job_id).and_then(|j| j.gpu_ids),
            Some(GpuIds::from_iter([0, 1]))
        );
    }

    #[test]
    fn test_shared_jobs_can_share_same_gpu() {
        let mut scheduler = create_test_scheduler();
        scheduler.gpu_slots.insert(
            "GPU-0".to_string(),
            GPUSlot {
                index: 0,
                available: true,
                total_memory_mb: None,
                reason: None,
            },
        );

        let job_a = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        let prepared_a = scheduler.prepare_jobs_for_execution();
        assert_eq!(prepared_a.len(), 1);
        assert_eq!(prepared_a[0].id, job_a_id);
        assert_eq!(
            scheduler.get_job(job_a_id).and_then(|j| j.gpu_ids),
            Some(GpuIds::from_iter([0]))
        );

        let job_b = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        let prepared_b = scheduler.prepare_jobs_for_execution();
        assert_eq!(prepared_b.len(), 1);
        assert_eq!(prepared_b[0].id, job_b_id);
        assert_eq!(
            scheduler.get_job(job_b_id).and_then(|j| j.gpu_ids),
            Some(GpuIds::from_iter([0]))
        );
    }

    #[test]
    fn test_exclusive_job_waits_when_shared_job_is_running() {
        let mut scheduler = create_test_scheduler();
        scheduler.gpu_slots.insert(
            "GPU-0".to_string(),
            GPUSlot {
                index: 0,
                available: true,
                total_memory_mb: None,
                reason: None,
            },
        );

        let shared_job = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .build();
        let (shared_job_id, _) = scheduler.submit_job(shared_job);
        scheduler.prepare_jobs_for_execution();

        let exclusive_job = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .build();
        let (exclusive_job_id, _) = scheduler.submit_job(exclusive_job);

        let prepared = scheduler.prepare_jobs_for_execution();
        assert!(prepared.is_empty());
        assert_eq!(
            scheduler.get_job(exclusive_job_id).map(|j| j.state),
            Some(JobState::Queued)
        );
        assert_eq!(
            scheduler
                .get_job(exclusive_job_id)
                .and_then(|j| j.reason.map(|r| *r)),
            Some(JobStateReason::WaitingForGpu)
        );

        scheduler.finish_job(shared_job_id);
        let prepared_after_finish = scheduler.prepare_jobs_for_execution();
        assert_eq!(prepared_after_finish.len(), 1);
        assert_eq!(prepared_after_finish[0].id, exclusive_job_id);
        assert_eq!(scheduler.get_job(exclusive_job_id).unwrap().reason, None);
    }

    #[test]
    fn test_shared_job_can_still_schedule_after_one_shared_job_finishes() {
        let mut scheduler = create_test_scheduler();
        scheduler.gpu_slots.insert(
            "GPU-0".to_string(),
            GPUSlot {
                index: 0,
                available: true,
                total_memory_mb: None,
                reason: None,
            },
        );

        let job_a = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        let job_b = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        let prepared = scheduler.prepare_jobs_for_execution();
        assert_eq!(prepared.len(), 2);
        assert_eq!(
            scheduler.get_job(job_b_id).map(|j| j.state),
            Some(JobState::Running)
        );

        scheduler.finish_job(job_a_id);

        let job_c = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        let prepared_c = scheduler.prepare_jobs_for_execution();
        assert_eq!(prepared_c.len(), 1);
        assert_eq!(prepared_c[0].id, job_c_id);
        assert_eq!(
            scheduler.get_job(job_c_id).and_then(|j| j.gpu_ids),
            Some(GpuIds::from_iter([0]))
        );
    }

    #[test]
    fn test_shared_jobs_respect_per_gpu_memory_limits() {
        let mut scheduler = create_test_scheduler();
        scheduler.gpu_slots.insert(
            "GPU-0".to_string(),
            GPUSlot {
                index: 0,
                available: true,
                total_memory_mb: Some(10_000),
                reason: None,
            },
        );

        let job_a = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .gpu_memory_limit_mb(Some(8_000))
            .build();
        scheduler.submit_job(job_a);
        let first = scheduler.prepare_jobs_for_execution();
        assert_eq!(first.len(), 1);

        let job_b = JobBuilder::new()
            .submitted_by("alice")
            .run_dir("/tmp")
            .gpus(1)
            .shared(true)
            .gpu_memory_limit_mb(Some(3_000))
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        // 8GB + 3GB > 10GB, so second shared job must wait.
        let second = scheduler.prepare_jobs_for_execution();
        assert!(second.is_empty());
        assert_eq!(
            scheduler.get_job(job_b_id).map(|j| j.state),
            Some(JobState::Queued)
        );
    }

    #[test]
    fn test_scheduler_info_includes_gpu_allocation_strategy() {
        let mut scheduler = create_test_scheduler();
        scheduler.set_gpu_allocation_strategy(GpuAllocationStrategy::Random);

        let info = scheduler.info();
        assert_eq!(info.gpu_allocation_strategy, GpuAllocationStrategy::Random);
    }

    #[test]
    fn test_group_running_count_updates_on_run_and_finish() {
        let mut scheduler = create_test_scheduler();
        let group_id = Uuid::new_v4();

        let job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .group_id_uuid(Some(group_id))
            .max_concurrent(Some(10))
            .build();
        let (job_id, _) = scheduler.submit_job(job);

        let jobs = scheduler.prepare_jobs_for_execution();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, job_id);
        assert_eq!(scheduler.group_running_count.get(&group_id), Some(&1));
        assert!(scheduler
            .state_jobs_index
            .get(&JobState::Running)
            .is_some_and(|v| v.contains(&job_id)));

        scheduler.finish_job(job_id).unwrap();
        assert!(!scheduler.group_running_count.contains_key(&group_id));
        assert!(scheduler
            .state_jobs_index
            .get(&JobState::Finished)
            .is_some_and(|v| v.contains(&job_id)));
    }

    #[test]
    fn test_resolve_dependency_most_recent() {
        let mut scheduler = create_test_scheduler();

        for _i in 0..3 {
            let job = JobBuilder::new()
                .submitted_by("alice")
                .run_dir("/tmp")
                .build();
            scheduler.submit_job(job);
        }

        assert_eq!(scheduler.resolve_dependency("alice", "@"), Some(3));
    }

    #[test]
    fn test_resolve_dependency_offset() {
        let mut scheduler = create_test_scheduler();

        for _i in 0..5 {
            let job = JobBuilder::new()
                .submitted_by("bob")
                .run_dir("/tmp")
                .build();
            scheduler.submit_job(job);
        }

        assert_eq!(scheduler.resolve_dependency("bob", "@~1"), Some(5));
        assert_eq!(scheduler.resolve_dependency("bob", "@~2"), Some(4));
        assert_eq!(scheduler.resolve_dependency("bob", "@~5"), Some(1));
        assert_eq!(scheduler.resolve_dependency("bob", "@~6"), None); // Out of range
    }

    #[test]
    fn test_calculate_time_bonus() {
        // No time limit
        assert_eq!(Scheduler::calculate_time_bonus(&None), 100);

        // 1 minute (very short)
        assert_eq!(
            Scheduler::calculate_time_bonus(&Some(Duration::from_secs(60))),
            299
        );

        // 24 hours (maximum)
        assert_eq!(
            Scheduler::calculate_time_bonus(&Some(Duration::from_secs(24 * 3600))),
            200
        );
    }

    #[test]
    fn test_refresh_available_memory() {
        let mut scheduler = create_test_scheduler();
        let total = scheduler.total_memory_mb;

        // Submit and "run" a job with memory limit
        let job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .memory_limit_mb(Some(1024))
            .build();
        let (job_id, _) = scheduler.submit_job(job);

        // Transition to Running (so indices stay consistent).
        assert!(scheduler
            .transition_job_state(job_id, JobState::Running, None)
            .unwrap());

        scheduler.refresh_available_memory();
        assert_eq!(scheduler.available_memory_mb, total - 1024);
    }

    #[test]
    fn test_prepare_jobs_refreshes_memory_after_finished_job() {
        let mut scheduler = create_test_scheduler();
        let total = scheduler.total_memory_mb;

        let first_job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .memory_limit_mb(Some(12 * 1024))
            .build();
        let (first_job_id, _) = scheduler.submit_job(first_job);

        let first_results = scheduler.prepare_jobs_for_execution();
        assert_eq!(first_results.len(), 1);
        assert_eq!(
            scheduler.get_job(first_job_id).unwrap().state,
            JobState::Running
        );
        assert_eq!(scheduler.available_memory_mb, total - 12 * 1024);

        scheduler.finish_job(first_job_id).unwrap();
        assert_eq!(
            scheduler.get_job(first_job_id).unwrap().state,
            JobState::Finished
        );

        let second_job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .memory_limit_mb(Some(8 * 1024))
            .build();
        let (second_job_id, _) = scheduler.submit_job(second_job);

        let second_results = scheduler.prepare_jobs_for_execution();
        assert_eq!(second_results.len(), 1);
        assert_eq!(second_results[0].id, second_job_id);
        assert_eq!(
            scheduler.get_job(second_job_id).unwrap().state,
            JobState::Running
        );
        assert_eq!(scheduler.available_memory_mb, total - 8 * 1024);
    }

    #[test]
    fn test_job_wait_reason_distinguishes_host_memory_pressure() {
        let mut scheduler = create_test_scheduler();

        let first_job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .memory_limit_mb(Some(12 * 1024))
            .build();
        let (first_job_id, _) = scheduler.submit_job(first_job);
        scheduler.prepare_jobs_for_execution();
        assert_eq!(
            scheduler.get_job(first_job_id).unwrap().state,
            JobState::Running
        );

        let second_job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .memory_limit_mb(Some(8 * 1024))
            .build();
        let (second_job_id, _) = scheduler.submit_job(second_job);

        let prepared = scheduler.prepare_jobs_for_execution();
        assert!(prepared.is_empty());
        assert_eq!(
            scheduler.get_job(second_job_id).map(|j| j.state),
            Some(JobState::Queued)
        );
        assert_eq!(
            scheduler
                .get_job(second_job_id)
                .and_then(|j| j.reason.map(|r| *r)),
            Some(JobStateReason::WaitingForMemory)
        );
    }

    #[test]
    fn test_state_jobs_index_updates_on_transitions() {
        let mut scheduler = create_test_scheduler();

        let job = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_id, _) = scheduler.submit_job(job);

        assert_eq!(
            scheduler.state_jobs_index.get(&JobState::Queued).unwrap(),
            &vec![job_id]
        );

        assert!(scheduler
            .transition_job_state(job_id, JobState::Running, None)
            .unwrap());

        assert!(scheduler
            .state_jobs_index
            .get(&JobState::Queued)
            .is_none_or(|v| !v.contains(&job_id)));
        assert!(scheduler
            .state_jobs_index
            .get(&JobState::Running)
            .is_some_and(|v| v.contains(&job_id)));

        scheduler.finish_job(job_id).unwrap();
        assert!(scheduler
            .state_jobs_index
            .get(&JobState::Running)
            .is_none_or(|v| !v.contains(&job_id)));
        assert!(scheduler
            .state_jobs_index
            .get(&JobState::Finished)
            .is_some_and(|v| v.contains(&job_id)));

        // Rebuild indices should preserve state index contents.
        scheduler.rebuild_user_jobs_index();
        assert!(scheduler
            .state_jobs_index
            .get(&JobState::Finished)
            .is_some_and(|v| v.contains(&job_id)));
    }

    #[test]
    #[allow(deprecated)]
    fn test_schedule_jobs_without_executor_does_not_mutate_state() {
        // Create scheduler WITHOUT executor (simulating misconfiguration)
        let mut scheduler = SchedulerBuilder::new()
            .with_state_path(PathBuf::from("/tmp/test.json"))
            .with_total_memory_mb(16 * 1024)
            .build();

        // Submit a job
        let job = create_test_job("test");
        let (job_id, _) = scheduler.submit_job(job);

        // Verify job is Queued
        assert_eq!(scheduler.get_job(job_id).unwrap().state, JobState::Queued);
        let initial_available_memory = scheduler.available_memory_mb;

        // Try to schedule jobs without executor
        let results = scheduler.schedule_jobs();

        // Should return empty results
        assert_eq!(results.len(), 0);

        // Job should STILL be Queued (not stuck in Running)
        assert_eq!(
            scheduler.get_job(job_id).unwrap().state,
            JobState::Queued,
            "Job should remain Queued when no executor is present"
        );

        // Memory should not be allocated
        assert_eq!(
            scheduler.available_memory_mb, initial_available_memory,
            "Memory should not be allocated when no executor is present"
        );

        // GPU IDs should not be assigned
        assert_eq!(
            scheduler.get_job(job_id).unwrap().gpu_ids,
            None,
            "GPU IDs should not be assigned when no executor is present"
        );

        // started_at should not be set
        assert_eq!(
            scheduler.get_job(job_id).unwrap().started_at,
            None,
            "started_at should not be set when no executor is present"
        );
    }

    #[test]
    fn test_auto_cancel_direct_dependent() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A with auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        scheduler.transition_job_state(job_a_id, JobState::Running, None);
        // Fail job A
        scheduler.fail_job(job_a_id);
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().reason,
            Some(Box::new(JobStateReason::DependencyFailed(job_a_id)))
        );
    }

    #[test]
    fn test_auto_cancel_transitive_dependencies() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A with auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        // Create job C that depends on B with auto_cancel enabled
        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        scheduler.transition_job_state(job_a_id, JobState::Running, None);
        // Fail job A
        scheduler.fail_job(job_a_id);
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_c_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().reason,
            Some(Box::new(JobStateReason::DependencyFailed(job_a_id)))
        );
        assert_eq!(
            scheduler.get_job(job_c_id).unwrap().reason,
            Some(Box::new(JobStateReason::DependencyFailed(job_b_id)))
        );
    }

    #[test]
    fn test_auto_cancel_deep_chain() {
        let mut scheduler = create_test_scheduler();

        // Create a chain: A -> B -> C -> D -> E
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        let job_d = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_c_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_d_id, _) = scheduler.submit_job(job_d);

        let job_e = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_d_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_e_id, _) = scheduler.submit_job(job_e);

        scheduler.transition_job_state(job_a_id, JobState::Running, None);
        // Fail job A
        scheduler.fail_job(job_a_id);
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_c_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_d_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get_job(job_e_id).unwrap().state,
            JobState::Cancelled
        );
    }

    #[test]
    fn test_auto_cancel_respects_flag() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A but WITHOUT auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(false)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        scheduler.transition_job_state(job_a_id, JobState::Running, None);
        // Fail job A
        scheduler.fail_job(job_a_id);
        assert_eq!(scheduler.get_job(job_b_id).unwrap().state, JobState::Queued);
    }

    #[test]
    fn test_auto_cancel_mixed_flags() {
        let mut scheduler = create_test_scheduler();

        // Create job A
        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        // Create job B that depends on A with auto_cancel enabled
        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id])
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        // Create job C that depends on B WITHOUT auto_cancel enabled
        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_b_id])
            .auto_cancel_on_dependency_failure(false)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        scheduler.transition_job_state(job_a_id, JobState::Running, None);
        // Fail job A
        scheduler.fail_job(job_a_id);
        assert_eq!(
            scheduler.get_job(job_b_id).unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(scheduler.get_job(job_c_id).unwrap().state, JobState::Queued);
    }

    #[test]
    fn test_auto_cancel_waits_for_any_mode_to_become_impossible() {
        let mut scheduler = create_test_scheduler();

        let job_a = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_a_id, _) = scheduler.submit_job(job_a);

        let job_b = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .build();
        let (job_b_id, _) = scheduler.submit_job(job_b);

        let job_c = JobBuilder::new()
            .submitted_by("test")
            .run_dir("/tmp")
            .depends_on_ids(vec![job_a_id, job_b_id])
            .dependency_mode(Some(DependencyMode::Any))
            .auto_cancel_on_dependency_failure(true)
            .build();
        let (job_c_id, _) = scheduler.submit_job(job_c);

        scheduler.transition_job_state(job_a_id, JobState::Running, None);
        scheduler.fail_job(job_a_id);
        assert_eq!(scheduler.get_job(job_c_id).unwrap().state, JobState::Queued);

        scheduler.transition_job_state(job_b_id, JobState::Running, None);
        scheduler.fail_job(job_b_id);
        assert_eq!(
            scheduler.get_job(job_c_id).unwrap().state,
            JobState::Cancelled
        );
    }

    #[test]
    fn test_create_reservation_with_indices() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add some GPU slots
        for i in 0..4 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    total_memory_mb: None,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Create reservation with specific GPU indices
        let result = scheduler.create_reservation(
            "alice".into(),
            GpuSpec::Indices(vec![0, 2]),
            start_time,
            duration,
        );

        assert!(result.is_ok());
        let reservation_id = result.unwrap();
        assert_eq!(reservation_id, 1);

        let reservation = scheduler.get_reservation(reservation_id).unwrap();
        assert_eq!(reservation.user, "alice");
        assert_eq!(reservation.gpu_spec, GpuSpec::Indices(vec![0, 2]));
        assert_eq!(reservation.gpu_spec.count(), 2);
    }

    #[test]
    fn test_reservation_conflict_indices_vs_indices() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add GPU slots
        for i in 0..4 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    total_memory_mb: None,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Create first reservation for GPU 0, 1
        scheduler
            .create_reservation(
                "alice".into(),
                GpuSpec::Indices(vec![0, 1]),
                start_time,
                duration,
            )
            .unwrap();

        // Try to create overlapping reservation for GPU 1, 2 (should fail due to GPU 1 conflict)
        let result = scheduler.create_reservation(
            "bob".into(),
            GpuSpec::Indices(vec![1, 2]),
            start_time,
            duration,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("GPU index 1 is already reserved"));

        // Create non-overlapping reservation for GPU 2, 3 (should succeed)
        let result = scheduler.create_reservation(
            "bob".into(),
            GpuSpec::Indices(vec![2, 3]),
            start_time,
            duration,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_reservation_conflict_count_vs_indices() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add 4 GPU slots
        for i in 0..4 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    total_memory_mb: None,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Create index-based reservation for GPU 0, 1
        scheduler
            .create_reservation(
                "alice".into(),
                GpuSpec::Indices(vec![0, 1]),
                start_time,
                duration,
            )
            .unwrap();

        // Try to create count-based reservation for 3 GPUs (should fail - only 2 unreserved GPUs left)
        let result =
            scheduler.create_reservation("bob".into(), GpuSpec::Count(3), start_time, duration);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("conflicts"));

        // Create count-based reservation for 2 GPUs (should succeed - GPUs 2, 3 available)
        let result =
            scheduler.create_reservation("bob".into(), GpuSpec::Count(2), start_time, duration);

        assert!(result.is_ok());
    }

    #[test]
    fn test_reservation_out_of_range_index() {
        use crate::core::reservation::GpuSpec;
        let mut scheduler = create_test_scheduler();

        // Add only 2 GPU slots
        for i in 0..2 {
            scheduler.gpu_slots.insert(
                format!("GPU-{}", i),
                GPUSlot {
                    index: i,
                    available: true,
                    total_memory_mb: None,
                    reason: None,
                },
            );
        }

        let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let duration = std::time::Duration::from_secs(7200);

        // Try to reserve GPU index 3 (out of range)
        let result = scheduler.create_reservation(
            "alice".into(),
            GpuSpec::Indices(vec![0, 3]),
            start_time,
            duration,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("GPU index 3 is out of range"));
    }

    // Property-based tests for GPU allocation invariants
    mod proptests {
        use super::*;
        use crate::core::reservation::GpuSpec;
        use proptest::prelude::*;

        // Helper to create a scheduler with N GPUs
        fn scheduler_with_gpus(n: u32) -> Scheduler {
            let mut scheduler = create_test_scheduler();
            for i in 0..n {
                scheduler.gpu_slots.insert(
                    format!("GPU-{}", i),
                    GPUSlot {
                        index: i,
                        available: true,
                        total_memory_mb: None,
                        reason: None,
                    },
                );
            }
            scheduler
        }

        proptest! {
            /// Property: Total allocated GPUs never exceeds system total
            /// Create multiple reservations and verify no over-allocation
            #[test]
            fn prop_no_gpu_overallocation(
                total_gpus in 2u32..8,
                reservation_count in 1usize..5,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                let mut successful_reservations = Vec::new();

                // Try to create multiple reservations
                for i in 0..reservation_count {
                    let gpu_count = (i as u32 % total_gpus) + 1;
                    let result = scheduler.create_reservation(
                        format!("user{}", i).into(),
                        GpuSpec::Count(gpu_count),
                        start_time,
                        duration,
                    );

                    if result.is_ok() {
                        successful_reservations.push(gpu_count);
                    }
                }

                // Verify: sum of successful reservations <= total_gpus
                let total_allocated: u32 = successful_reservations.iter().sum();
                prop_assert!(total_allocated <= total_gpus);
            }

            /// Property: Index-based reservations never have overlapping indices
            #[test]
            fn prop_no_index_overlap(
                total_gpus in 4u32..8,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Create first reservation with indices [0, 1]
                let res1 = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Indices(vec![0, 1]),
                    start_time,
                    duration,
                );
                prop_assert!(res1.is_ok());

                // Try to create second reservation with overlapping index [1, 2]
                let res2 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Indices(vec![1, 2]),
                    start_time,
                    duration,
                );
                // Should fail due to index 1 conflict
                prop_assert!(res2.is_err());

                // Create third reservation with non-overlapping indices [2, 3]
                let res3 = scheduler.create_reservation(
                    "charlie".into(),
                    GpuSpec::Indices(vec![2, 3]),
                    start_time,
                    duration,
                );
                // Should succeed
                prop_assert!(res3.is_ok());
            }

            /// Property: Count-based reservations respect index-based reservations
            #[test]
            fn prop_count_respects_indices(
                total_gpus in 4u32..8,
                reserved_indices_count in 1u32..3,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Create index-based reservation
                let indices: Vec<u32> = (0..reserved_indices_count).collect();
                scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Indices(indices),
                    start_time,
                    duration,
                ).unwrap();

                let available_for_count = total_gpus - reserved_indices_count;

                // Try to reserve exactly the available count (should succeed)
                let res1 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Count(available_for_count),
                    start_time,
                    duration,
                );
                prop_assert!(res1.is_ok());

                // Try to reserve one more (should fail)
                let res2 = scheduler.create_reservation(
                    "charlie".into(),
                    GpuSpec::Count(1),
                    start_time,
                    duration,
                );
                prop_assert!(res2.is_err());
            }

            /// Property: Non-overlapping time ranges never conflict
            #[test]
            fn prop_no_conflict_different_times(
                total_gpus in 2u32..8,
                gpu_count in 1u32..4,
                time_gap in 1u64..1000,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let gpu_count = std::cmp::min(gpu_count, total_gpus);

                let start1 = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration1 = std::time::Duration::from_secs(7200);

                // Create first reservation
                let res1 = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(gpu_count),
                    start1,
                    duration1,
                );
                prop_assert!(res1.is_ok());

                // Create second reservation starting after first ends
                let start2 = start1 + duration1 + std::time::Duration::from_secs(time_gap);
                let duration2 = std::time::Duration::from_secs(3600);

                let res2 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Count(gpu_count),
                    start2,
                    duration2,
                );
                // Should succeed since time ranges don't overlap
                prop_assert!(res2.is_ok());
            }

            /// Property: Cancelling a reservation frees up resources
            #[test]
            fn prop_cancel_frees_resources(
                total_gpus in 2u32..8,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Reserve all GPUs
                let res1_id = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(total_gpus),
                    start_time,
                    duration,
                ).unwrap();

                // Try to create another reservation (should fail)
                let res2 = scheduler.create_reservation(
                    "bob".into(),
                    GpuSpec::Count(1),
                    start_time,
                    duration,
                );
                prop_assert!(res2.is_err());

                // Cancel first reservation
                scheduler.cancel_reservation(res1_id).unwrap();

                // Now should be able to create new reservation
                let res3 = scheduler.create_reservation(
                    "charlie".into(),
                    GpuSpec::Count(total_gpus),
                    start_time,
                    duration,
                );
                prop_assert!(res3.is_ok());
            }

            /// Property: Invalid GPU indices are always rejected
            #[test]
            fn prop_reject_invalid_indices(
                total_gpus in 2u32..8,
                invalid_index in 8u32..100,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                // Try to reserve an out-of-range GPU index
                let result = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Indices(vec![0, invalid_index]),
                    start_time,
                    duration,
                );

                prop_assert!(result.is_err());
                prop_assert!(result.unwrap_err().to_string().contains("out of range"));
            }

            /// Property: Zero GPU count is always rejected
            #[test]
            fn prop_reject_zero_gpus(
                total_gpus in 2u32..8,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                let result = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(0),
                    start_time,
                    duration,
                );

                prop_assert!(result.is_err());
                prop_assert!(result.unwrap_err().to_string().contains("must be greater than 0"));
            }

            /// Property: Requesting more GPUs than available is always rejected
            #[test]
            fn prop_reject_excessive_gpus(
                total_gpus in 2u32..8,
                extra in 1u32..10,
            ) {
                let mut scheduler = scheduler_with_gpus(total_gpus);
                let start_time = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
                let duration = std::time::Duration::from_secs(7200);

                let excessive_count = total_gpus + extra;
                let result = scheduler.create_reservation(
                    "alice".into(),
                    GpuSpec::Count(excessive_count),
                    start_time,
                    duration,
                );

                prop_assert!(result.is_err());
            }
        }
    }
}
