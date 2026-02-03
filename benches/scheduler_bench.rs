//! Benchmarks for gflow scheduler performance at scale (10k-100k jobs)
//!
//! This benchmark suite measures:
//! - Memory consumption with varying job counts
//! - Query performance (list, filter, lookup)
//! - Job submission throughput
//! - Scheduling decision performance

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use gflow::core::job::{DependencyMode, Job, JobBuilder, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use gflow::core::GPUSlot;
use std::collections::HashMap;
use std::hint::black_box as hint_black_box;
use std::path::PathBuf;
use std::time::Duration;

/// Create a test job with realistic fields populated
fn create_test_job(index: u32) -> Job {
    JobBuilder::new()
        .submitted_by(format!("user{}", index % 100))
        .run_dir(format!(
            "/home/user{}/work/project{}",
            index % 100,
            index % 1000
        ))
        .command(format!(
            "python train.py --lr 0.001 --epochs {} --batch-size 32",
            index % 100
        ))
        .gpus(index % 4)
        .priority((index % 20) as u8)
        .time_limit(Some(Duration::from_secs((index % 24 + 1) as u64 * 3600)))
        .memory_limit_mb(Some((index % 16 + 1) as u64 * 1024))
        .conda_env(Some(format!("env{}", index % 10)))
        .auto_close_tmux(index.is_multiple_of(2))
        .build()
}

/// Create a job with dependencies
fn create_job_with_deps(index: u32, deps: Vec<u32>) -> Job {
    JobBuilder::new()
        .submitted_by(format!("user{}", index % 100))
        .run_dir(format!("/home/user{}/work", index % 100))
        .command(format!("python script{}.py", index))
        .gpus(index % 2)
        .priority((index % 20) as u8)
        .depends_on_ids(deps)
        .dependency_mode(Some(DependencyMode::All))
        .auto_cancel_on_dependency_failure(true)
        .build()
}

/// Create a job with parameters (realistic ML training scenario)
fn create_job_with_params(index: u32) -> Job {
    let mut params = HashMap::new();
    params.insert(
        "lr".to_string(),
        format!("{:.4}", (index % 100) as f64 / 10000.0),
    );
    params.insert("epochs".to_string(), format!("{}", (index % 50) + 10));
    params.insert(
        "batch_size".to_string(),
        format!("{}", 32 * (1 << (index % 4))),
    );
    params.insert(
        "model".to_string(),
        format!("resnet{}", 18 * (1 << (index % 3))),
    );
    params.insert(
        "optimizer".to_string(),
        if index.is_multiple_of(2) {
            "adam".to_string()
        } else {
            "sgd".to_string()
        },
    );
    params.insert("seed".to_string(), format!("{}", index));

    JobBuilder::new()
        .submitted_by(format!("user{}", index % 100))
        .run_dir(format!("/home/user{}/experiments", index % 100))
        .command("python train.py --lr {lr} --epochs {epochs} --batch-size {batch_size} --model {model} --optimizer {optimizer} --seed {seed}".to_string())
        .gpus(index % 4)
        .priority((index % 20) as u8)
        .parameters(params)
        .conda_env(Some(format!("ml-env{}", index % 5)))
        .build()
}

/// Create a scheduler with GPU slots for testing
fn create_test_scheduler() -> Scheduler {
    let mut gpu_slots = HashMap::new();
    for i in 0..8 {
        gpu_slots.insert(
            format!("GPU-{}", i),
            GPUSlot {
                index: i,
                available: true,
                reason: None,
            },
        );
    }

    SchedulerBuilder::new()
        .with_gpu_slots(gpu_slots)
        .with_state_path(PathBuf::from("/tmp/bench_state.json"))
        .with_total_memory_mb(128 * 1024) // 128GB
        .build()
}

/// Populate scheduler with N jobs
fn populate_scheduler(scheduler: &mut Scheduler, count: usize) {
    for i in 0..count {
        let job = create_test_job(i as u32);
        scheduler.submit_job(job);
    }
}

/// Populate scheduler with jobs that have dependencies (chain pattern)
fn populate_scheduler_with_deps(scheduler: &mut Scheduler, count: usize) {
    // First 10% are root jobs (no dependencies)
    let root_count = count / 10;
    for i in 0..root_count {
        let job = create_test_job(i as u32);
        scheduler.submit_job(job);
    }

    // Remaining 90% depend on previous jobs
    for i in root_count..count {
        let dep_id = (i % root_count) as u32 + 1; // Depend on one of the root jobs
        let job = create_job_with_deps(i as u32, vec![dep_id]);
        scheduler.submit_job(job);
    }
}

// ============================================================================
// Memory Consumption Benchmarks
// ============================================================================

fn bench_memory_job_storage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory/job_storage");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter(|| {
                let mut scheduler = create_test_scheduler();
                populate_scheduler(&mut scheduler, size);
                hint_black_box(&scheduler);
            });
        });
    }

    group.finish();
}

// ============================================================================
// Bottleneck Analysis Benchmarks
// ============================================================================

/// Benchmark job creation into scheduler storage.
///
/// This measures the performance of `Scheduler::submit_job` at scale (10k-100k),
/// which is the path exercised when creating large numbers of jobs in real deployments.
fn bench_job_creation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/job_creation");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        // Pre-create all jobs so we measure scheduler insertion / storage, not job construction.
        let jobs: Vec<Job> = (0..size).map(|i| create_test_job(i as u32)).collect();
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &jobs, |b, jobs| {
            b.iter(|| {
                let mut scheduler = create_test_scheduler();
                for job in jobs.iter() {
                    std::hint::black_box(scheduler.submit_job(job.clone()));
                }
                hint_black_box(&scheduler);
            });
        });
    }

    group.finish();
}

/// Benchmark Job creation only (drop happens outside the timed section).
///
/// This helps identify whether large-N slowdowns come from construction vs. teardown.
fn bench_job_create_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/job_create_only");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || (),
                |_| {
                    let jobs: Vec<Job> = (0..size).map(|i| create_test_job(i as u32)).collect();
                    hint_black_box(jobs)
                },
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

/// Benchmark dropping a pre-built Vec<Job> (construction happens outside the timed section).
///
/// Note: the input Vec is created immediately before dropping (during setup),
/// so it may be "warm" in cache; still useful to compare scaling behavior.
fn bench_job_drop_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/job_drop_only");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let jobs: Vec<Job> = (0..size).map(|i| create_test_job(i as u32)).collect();
                    hint_black_box(jobs)
                },
                |jobs| {
                    drop(jobs);
                },
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

/// Benchmark Job clone cost
fn bench_job_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/job_clone");

    let job = create_test_job(12345);

    group.bench_function("single_clone", |b| {
        b.iter(|| std::hint::black_box(job.clone()));
    });

    group.finish();
}

/// Benchmark String allocation in Job creation
fn bench_string_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/string_alloc");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter(|| {
                let strings: Vec<(String, String, String, String)> = (0..size)
                    .map(|i| {
                        (
                            format!("user{}", i % 100),
                            format!("/home/user{}/work/project{}", i % 100, i % 1000),
                            format!(
                                "python train.py --lr 0.001 --epochs {} --batch-size 32",
                                i % 100
                            ),
                            format!("env{}", i % 10),
                        )
                    })
                    .collect();
                hint_black_box(strings);
            });
        });
    }

    group.finish();
}

/// Benchmark job creation with parameters (realistic ML training scenario)
fn bench_job_creation_with_params(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/job_creation_with_params");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter(|| {
                let jobs: Vec<Job> = (0..size)
                    .map(|i| create_job_with_params(i as u32))
                    .collect();
                hint_black_box(jobs);
            });
        });
    }

    group.finish();
}

/// Benchmark parameter string allocation specifically
fn bench_parameter_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/parameter_alloc");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter(|| {
                let params_vec: Vec<HashMap<String, String>> = (0..size)
                    .map(|i| {
                        let mut params = HashMap::new();
                        params.insert(
                            "lr".to_string(),
                            format!("{:.4}", (i % 100) as f64 / 10000.0),
                        );
                        params.insert("epochs".to_string(), format!("{}", (i % 50) + 10));
                        params.insert("batch_size".to_string(), format!("{}", 32 * (1 << (i % 4))));
                        params.insert(
                            "model".to_string(),
                            format!("resnet{}", 18 * (1 << (i % 3))),
                        );
                        params.insert(
                            "optimizer".to_string(),
                            if i % 2 == 0 {
                                "adam".to_string()
                            } else {
                                "sgd".to_string()
                            },
                        );
                        params.insert("seed".to_string(), format!("{}", i));
                        params
                    })
                    .collect();
                hint_black_box(params_vec);
            });
        });
    }

    group.finish();
}

/// Benchmark job cloning with parameters
fn bench_job_clone_with_params(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/job_clone_with_params");

    let job = create_job_with_params(12345);

    group.bench_function("single_clone", |b| {
        b.iter(|| std::hint::black_box(job.clone()));
    });

    group.finish();
}

fn bench_memory_with_dependencies(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory/with_dependencies");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter(|| {
                let mut scheduler = create_test_scheduler();
                populate_scheduler_with_deps(&mut scheduler, size);
                hint_black_box(&scheduler);
            });
        });
    }

    group.finish();
}

// ============================================================================
// Job Submission Benchmarks
// ============================================================================

fn bench_job_submission(c: &mut Criterion) {
    let mut group = c.benchmark_group("submission/single");

    // Benchmark single job submission with varying existing job counts
    for existing_jobs in [0, 10_000, 50_000, 100_000] {
        group.bench_with_input(
            BenchmarkId::new("existing_jobs", existing_jobs),
            &existing_jobs,
            |b, &existing_jobs| {
                let mut scheduler = create_test_scheduler();
                populate_scheduler(&mut scheduler, existing_jobs);

                b.iter(|| {
                    let job = create_test_job(existing_jobs as u32 + 1);
                    std::hint::black_box(scheduler.submit_job(job));
                });
            },
        );
    }

    group.finish();
}

fn bench_batch_submission(c: &mut Criterion) {
    let mut group = c.benchmark_group("submission/batch");
    group.sample_size(10);

    // Benchmark batch submission of 1000 jobs
    for existing_jobs in [0, 10_000, 50_000, 100_000] {
        group.throughput(Throughput::Elements(1000));
        group.bench_with_input(
            BenchmarkId::new("existing_jobs", existing_jobs),
            &existing_jobs,
            |b, &existing_jobs| {
                b.iter_batched(
                    || {
                        let mut scheduler = create_test_scheduler();
                        populate_scheduler(&mut scheduler, existing_jobs);
                        scheduler
                    },
                    |mut scheduler| {
                        for i in 0..1000 {
                            let job = create_test_job(existing_jobs as u32 + i + 1);
                            std::hint::black_box(scheduler.submit_job(job));
                        }
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ============================================================================
// Query Benchmarks
// ============================================================================

fn bench_query_all_jobs(c: &mut Criterion) {
    let mut group = c.benchmark_group("query/all_jobs");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler(&mut scheduler, size);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| {
                    let ids: Vec<u32> = scheduler.job_runtimes().iter().map(|rt| rt.id).collect();
                    std::hint::black_box(ids.len())
                });
            },
        );
    }

    group.finish();
}

fn bench_query_by_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("query/by_state");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler(&mut scheduler, size);

        // Set some jobs to different states for realistic distribution
        for i in 0..size {
            let job_id = i as u32 + 1;
            if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                match i % 5 {
                    0 => rt.state = JobState::Running,
                    1 => rt.state = JobState::Finished,
                    2 => rt.state = JobState::Failed,
                    3 => rt.state = JobState::Hold,
                    _ => {} // Keep as Queued
                }
            }
        }

        // Rebuild indices (including state index) after direct state mutation.
        scheduler.rebuild_user_jobs_index();

        group.bench_with_input(
            BenchmarkId::new("scan", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| {
                    let mut sum = 0u32;
                    for rt in scheduler
                        .job_runtimes()
                        .iter()
                        .filter(|rt| rt.state == JobState::Queued)
                    {
                        sum = sum.wrapping_add(rt.id);
                    }
                    hint_black_box(sum)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("index", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| {
                    let mut sum = 0u32;
                    for &job_id in scheduler.job_ids_by_state(JobState::Queued).unwrap_or(&[]) {
                        sum = sum.wrapping_add(job_id);
                    }
                    hint_black_box(sum)
                });
            },
        );
    }

    group.finish();
}

fn bench_query_by_user(c: &mut Criterion) {
    let mut group = c.benchmark_group("query/by_user");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler(&mut scheduler, size);

        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| {
                    let user_jobs = scheduler.get_jobs_by_user("user42");
                    std::hint::black_box(user_jobs.len())
                });
            },
        );
    }

    group.finish();
}

fn bench_query_single_job(c: &mut Criterion) {
    let mut group = c.benchmark_group("query/single_job");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler(&mut scheduler, size);

        let target_id = (size / 2) as u32; // Query job in the middle

        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &(scheduler, target_id),
            |b, (scheduler, target_id)| {
                b.iter(|| std::hint::black_box(scheduler.get_job(*target_id)));
            },
        );
    }

    group.finish();
}

// ============================================================================
// Dependency Resolution Benchmarks
// ============================================================================

fn bench_resolve_dependency(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency/resolve");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler(&mut scheduler, size);

        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| {
                    // Resolve "@" (most recent job by user)
                    std::hint::black_box(scheduler.resolve_dependency("user42", "@"))
                });
            },
        );
    }

    group.finish();
}

fn bench_validate_circular_dependency(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency/validate_circular");
    group.sample_size(50);

    for size in [10_000, 25_000, 50_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler_with_deps(&mut scheduler, size);

        let new_job_id = size as u32 + 1;
        let deps = vec![1, 2, 3]; // Depend on first few jobs

        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &(scheduler, new_job_id, deps),
            |b, (scheduler, new_job_id, deps)| {
                b.iter(|| {
                    std::hint::black_box(
                        scheduler.validate_no_circular_dependency(*new_job_id, deps),
                    )
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Scheduling Decision Benchmarks
// ============================================================================

fn bench_get_available_gpu_slots(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduling/available_gpus");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler(&mut scheduler, size);

        // Mark some GPUs as unavailable
        for (i, slot) in scheduler.gpu_slots_mut().values_mut().enumerate() {
            slot.available = i % 2 == 0;
        }

        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| std::hint::black_box(scheduler.get_available_gpu_slots()));
            },
        );
    }

    group.finish();
}

fn bench_refresh_available_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduling/refresh_memory");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    populate_scheduler(&mut scheduler, size);

                    // Set some jobs to Running state
                    for i in 0..size {
                        if i % 10 == 0 {
                            let job_id = i as u32 + 1;
                            if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                                rt.state = JobState::Running;
                            }
                        }
                    }
                    scheduler
                },
                |mut scheduler| {
                    scheduler.refresh_available_memory();
                    std::hint::black_box(scheduler.available_memory_mb())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_job_counts_by_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduling/job_counts");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        let mut scheduler = create_test_scheduler();
        populate_scheduler(&mut scheduler, size);

        // Set jobs to different states
        for i in 0..size {
            let job_id = i as u32 + 1;
            if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                match i % 5 {
                    0 => rt.state = JobState::Running,
                    1 => rt.state = JobState::Finished,
                    2 => rt.state = JobState::Failed,
                    3 => rt.state = JobState::Hold,
                    _ => {}
                }
            }
        }

        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| std::hint::black_box(scheduler.get_job_counts_by_state()));
            },
        );
    }

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    memory_benches,
    bench_memory_job_storage,
    bench_memory_with_dependencies,
);

criterion_group!(
    bottleneck_benches,
    bench_job_creation_only,
    bench_job_create_only,
    bench_job_drop_only,
    bench_job_clone,
    bench_string_allocation,
    bench_job_creation_with_params,
    bench_parameter_allocation,
    bench_job_clone_with_params,
);

criterion_group!(
    submission_benches,
    bench_job_submission,
    bench_batch_submission,
);

criterion_group!(
    query_benches,
    bench_query_all_jobs,
    bench_query_by_state,
    bench_query_by_user,
    bench_query_single_job,
);

criterion_group!(
    dependency_benches,
    bench_resolve_dependency,
    bench_validate_circular_dependency,
);

criterion_group!(
    scheduling_benches,
    bench_get_available_gpu_slots,
    bench_refresh_available_memory,
    bench_job_counts_by_state,
);

criterion_main!(
    memory_benches,
    bottleneck_benches,
    submission_benches,
    query_benches,
    dependency_benches,
    scheduling_benches,
    group_concurrency_benches,
    state_transition_benches,
    scheduling_flow_benches,
    auto_cancel_benches,
    reservation_benches,
);

// ============================================================================
// Group Concurrency Benchmarks (Issue #72)
// ============================================================================

/// Create jobs with group_id and max_concurrent for testing group concurrency checks
fn populate_scheduler_with_groups(scheduler: &mut Scheduler, count: usize, groups: usize) {
    use uuid::Uuid;

    // Create a few group IDs
    let group_ids: Vec<Uuid> = (0..groups).map(|_| Uuid::new_v4()).collect();

    for i in 0..count {
        let group_id = group_ids[i % groups];
        let job = JobBuilder::new()
            .submitted_by(format!("user{}", i % 100))
            .run_dir(format!("/home/user{}/work", i % 100))
            .command(format!("python script{}.py", i))
            .gpus((i % 4) as u32)
            .priority((i % 20) as u8)
            .group_id_uuid(Some(group_id))
            .max_concurrent(Some(10)) // Limit to 10 concurrent jobs per group
            .build();
        scheduler.submit_job(job);
    }

    // Set some jobs to Running state to trigger group concurrency checks
    let jobs_per_group = count / groups;
    for group_idx in 0..groups {
        let start_idx = group_idx * jobs_per_group;
        for i in 0..5 {
            let job_id = (start_idx + i + 1) as u32;
            if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                rt.state = JobState::Running;
                rt.started_at = Some(std::time::SystemTime::now());
            }
        }
    }

    // Rebuild indices to ensure group_running_count is correct
    scheduler.rebuild_user_jobs_index();
}

/// Benchmark scheduling with group concurrency limits
/// This tests the O(1) group_running_count index vs O(n) scan
fn bench_group_concurrency_scheduling(c: &mut Criterion) {
    let mut group = c.benchmark_group("group_concurrency/scheduling");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    // Create jobs across 100 groups
                    populate_scheduler_with_groups(&mut scheduler, size, 100);
                    scheduler
                },
                |mut scheduler| {
                    // This will trigger group concurrency checks for all queued jobs
                    let jobs = scheduler.prepare_jobs_for_execution();
                    hint_black_box(jobs.len())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    group_concurrency_benches,
    bench_group_concurrency_scheduling
);

// ============================================================================
// State Transition Benchmarks
// ============================================================================

/// Benchmark job state transitions (finish, fail, cancel, hold, release)
fn bench_state_transitions(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_transitions/finish_job");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    populate_scheduler(&mut scheduler, size);
                    // Set some jobs to Running state so they can be finished
                    for i in 0..1000 {
                        let job_id = (i + 1) as u32;
                        if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                            rt.state = JobState::Running;
                            rt.started_at = Some(std::time::SystemTime::now());
                        }
                    }
                    scheduler
                },
                |mut scheduler| {
                    // Finish 100 jobs
                    for i in 0..100 {
                        let job_id = (i + 1) as u32;
                        hint_black_box(scheduler.finish_job(job_id));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_fail_job(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_transitions/fail_job");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    populate_scheduler(&mut scheduler, size);
                    // Set some jobs to Running state
                    for i in 0..1000 {
                        let job_id = (i + 1) as u32;
                        if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                            rt.state = JobState::Running;
                            rt.started_at = Some(std::time::SystemTime::now());
                        }
                    }
                    scheduler
                },
                |mut scheduler| {
                    // Fail 100 jobs
                    for i in 0..100 {
                        let job_id = (i + 1) as u32;
                        hint_black_box(scheduler.fail_job(job_id));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_cancel_job(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_transitions/cancel_job");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    populate_scheduler(&mut scheduler, size);
                    scheduler
                },
                |mut scheduler| {
                    // Cancel 100 jobs (can cancel from any state)
                    for i in 0..100 {
                        let job_id = (i + 1) as u32;
                        hint_black_box(scheduler.cancel_job(job_id, None));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_hold_release_job(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_transitions/hold_release");

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    populate_scheduler(&mut scheduler, size);
                    scheduler
                },
                |mut scheduler| {
                    // Hold and release 100 jobs
                    for i in 0..100 {
                        let job_id = (i + 1) as u32;
                        hint_black_box(scheduler.hold_job(job_id));
                        hint_black_box(scheduler.release_job(job_id));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    state_transition_benches,
    bench_state_transitions,
    bench_fail_job,
    bench_cancel_job,
    bench_hold_release_job,
);

// ============================================================================
// Complete Scheduling Decision Flow Benchmarks
// ============================================================================

/// Benchmark the complete prepare_jobs_for_execution flow
/// This is the core scheduling method that includes:
/// - Dependency checking
/// - Priority sorting
/// - Resource allocation (GPU, memory)
/// - Group concurrency checks
/// - Reservation checks
fn bench_complete_scheduling_flow(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduling_flow/complete");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    populate_scheduler(&mut scheduler, size);

                    // Set some jobs to Finished to satisfy dependencies
                    for i in 0..100 {
                        let job_id = (i + 1) as u32;
                        if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                            rt.state = JobState::Finished;
                        }
                    }

                    scheduler
                },
                |mut scheduler| {
                    let jobs = scheduler.prepare_jobs_for_execution();
                    hint_black_box(jobs.len())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark scheduling with complex dependencies
fn bench_scheduling_with_dependencies(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduling_flow/with_dependencies");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    populate_scheduler_with_deps(&mut scheduler, size);

                    // Mark root jobs as finished so dependent jobs can run
                    let root_count = size / 10;
                    for i in 0..root_count {
                        let job_id = (i + 1) as u32;
                        if let Some(rt) = scheduler.get_job_runtime_mut(job_id) {
                            rt.state = JobState::Finished;
                        }
                    }

                    scheduler
                },
                |mut scheduler| {
                    let jobs = scheduler.prepare_jobs_for_execution();
                    hint_black_box(jobs.len())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark scheduling with memory constraints
fn bench_scheduling_with_memory_limits(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduling_flow/memory_constrained");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();
                    // Set lower memory limit to create contention
                    scheduler.update_memory(32 * 1024); // 32GB

                    // Create jobs with varying memory requirements
                    for i in 0..size {
                        let mut job = create_test_job(i as u32);
                        // Some jobs need significant memory
                        job.memory_limit_mb = Some(((i % 8) + 1) as u64 * 2048); // 2-16GB
                        scheduler.submit_job(job);
                    }

                    scheduler
                },
                |mut scheduler| {
                    let jobs = scheduler.prepare_jobs_for_execution();
                    hint_black_box(jobs.len())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark scheduling with high priority variation
fn bench_scheduling_priority_sorting(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduling_flow/priority_sorting");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut scheduler = create_test_scheduler();

                    // Create jobs with wide priority distribution
                    for i in 0..size {
                        let mut job = create_test_job(i as u32);
                        job.priority = (i % 100) as u8; // 0-99 priority range
                        scheduler.submit_job(job);
                    }

                    scheduler
                },
                |mut scheduler| {
                    let jobs = scheduler.prepare_jobs_for_execution();
                    hint_black_box(jobs.len())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    scheduling_flow_benches,
    bench_complete_scheduling_flow,
    bench_scheduling_with_dependencies,
    bench_scheduling_with_memory_limits,
    bench_scheduling_priority_sorting,
);

// ============================================================================
// Auto-Cancel Dependent Jobs Benchmarks
// ============================================================================

/// Create a dependency chain: job1 -> job2 -> job3 -> ... -> jobN
fn create_dependency_chain(scheduler: &mut Scheduler, chain_length: usize) {
    for i in 0..chain_length {
        let mut job = create_test_job(i as u32);
        if i > 0 {
            // Each job depends on the previous one
            job.depends_on_ids = smallvec::smallvec![i as u32];
            job.auto_cancel_on_dependency_failure = true;
        }
        scheduler.submit_job(job);
    }
}

/// Create a fan-out dependency pattern: one root job with many dependents
fn create_fan_out_dependencies(scheduler: &mut Scheduler, root_id: u32, fan_out_count: usize) {
    // Create root job
    let root_job = create_test_job(root_id);
    scheduler.submit_job(root_job);

    // Create dependent jobs
    for i in 0..fan_out_count {
        let mut job = create_test_job(root_id + i as u32 + 1);
        job.depends_on_ids = smallvec::smallvec![root_id];
        job.auto_cancel_on_dependency_failure = true;
        scheduler.submit_job(job);
    }
}

/// Create a multi-level dependency tree
fn create_dependency_tree(scheduler: &mut Scheduler, levels: usize, width: usize) {
    let mut current_level_jobs = vec![1u32];
    let mut next_job_id = 2u32;

    // Create root
    let root = create_test_job(1);
    scheduler.submit_job(root);

    // Create each level
    for _level in 0..levels {
        let mut next_level_jobs = Vec::new();

        for parent_id in &current_level_jobs {
            for _child in 0..width {
                let mut job = create_test_job(next_job_id);
                job.depends_on_ids = smallvec::smallvec![*parent_id];
                job.auto_cancel_on_dependency_failure = true;
                scheduler.submit_job(job);
                next_level_jobs.push(next_job_id);
                next_job_id += 1;
            }
        }

        current_level_jobs = next_level_jobs;
    }
}

/// Benchmark auto-cancel with chain dependencies
fn bench_auto_cancel_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("auto_cancel/chain");

    for chain_length in [100, 500, 1000, 2000] {
        group.bench_with_input(
            BenchmarkId::new("chain_length", chain_length),
            &chain_length,
            |b, &chain_length| {
                b.iter_batched(
                    || {
                        let mut scheduler = create_test_scheduler();
                        create_dependency_chain(&mut scheduler, chain_length);
                        scheduler
                    },
                    |mut scheduler| {
                        // Fail the first job, should cascade cancel all
                        scheduler.fail_job(1);
                        let cancelled = scheduler.auto_cancel_dependent_jobs(1);
                        hint_black_box(cancelled.len())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark auto-cancel with fan-out dependencies
fn bench_auto_cancel_fan_out(c: &mut Criterion) {
    let mut group = c.benchmark_group("auto_cancel/fan_out");

    for fan_out in [100, 500, 1000, 5000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("dependents", fan_out),
            &fan_out,
            |b, &fan_out| {
                b.iter_batched(
                    || {
                        let mut scheduler = create_test_scheduler();
                        create_fan_out_dependencies(&mut scheduler, 1, fan_out);
                        scheduler
                    },
                    |mut scheduler| {
                        // Fail the root job, should cancel all dependents
                        scheduler.fail_job(1);
                        let cancelled = scheduler.auto_cancel_dependent_jobs(1);
                        hint_black_box(cancelled.len())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark auto-cancel with tree dependencies
fn bench_auto_cancel_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("auto_cancel/tree");
    group.sample_size(10);

    // Test different tree shapes: (levels, width per level)
    for (levels, width) in [(3usize, 10usize), (4, 5), (5, 3), (6, 2)] {
        let _total_jobs = (0..levels).map(|l| width.pow(l as u32)).sum::<usize>() + 1;
        group.bench_with_input(
            BenchmarkId::new("tree", format!("L{}xW{}", levels, width)),
            &(levels, width),
            |b, &(levels, width)| {
                b.iter_batched(
                    || {
                        let mut scheduler = create_test_scheduler();
                        create_dependency_tree(&mut scheduler, levels, width);
                        scheduler
                    },
                    |mut scheduler| {
                        // Fail the root job, should cascade through entire tree
                        scheduler.fail_job(1);
                        let cancelled = scheduler.auto_cancel_dependent_jobs(1);
                        hint_black_box(cancelled.len())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark auto-cancel in a large scheduler with mixed dependencies
fn bench_auto_cancel_at_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("auto_cancel/at_scale");
    group.sample_size(10);

    for total_jobs in [10_000, 25_000, 50_000] {
        group.bench_with_input(
            BenchmarkId::new("total_jobs", total_jobs),
            &total_jobs,
            |b, &total_jobs| {
                b.iter_batched(
                    || {
                        let mut scheduler = create_test_scheduler();

                        // Create a mix of independent jobs and dependent jobs
                        // 10% are root jobs, 90% depend on roots
                        let root_count = total_jobs / 10;

                        // Create root jobs
                        for i in 0..root_count {
                            let job = create_test_job(i as u32);
                            scheduler.submit_job(job);
                        }

                        // Create dependent jobs
                        for i in root_count..total_jobs {
                            let mut job = create_test_job(i as u32);
                            let root_id = ((i % root_count) + 1) as u32;
                            job.depends_on_ids = smallvec::smallvec![root_id];
                            job.auto_cancel_on_dependency_failure = true;
                            scheduler.submit_job(job);
                        }

                        scheduler
                    },
                    |mut scheduler| {
                        // Fail a root job with many dependents
                        scheduler.fail_job(1);
                        let cancelled = scheduler.auto_cancel_dependent_jobs(1);
                        hint_black_box(cancelled.len())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    auto_cancel_benches,
    bench_auto_cancel_chain,
    bench_auto_cancel_fan_out,
    bench_auto_cancel_tree,
    bench_auto_cancel_at_scale,
);

// ============================================================================
// Reservation Operations Benchmarks
// ============================================================================

/// Benchmark creating GPU reservations
fn bench_create_reservation(c: &mut Criterion) {
    let mut group = c.benchmark_group("reservation/create");

    for existing_reservations in [0, 10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("existing", existing_reservations),
            &existing_reservations,
            |b, &existing_reservations| {
                b.iter_batched(
                    || {
                        use gflow::core::reservation::GpuSpec;
                        let mut scheduler = create_test_scheduler();

                        // Create existing reservations
                        for i in 0..existing_reservations {
                            let user = format!("user{}", i % 10);
                            let start = std::time::SystemTime::now()
                                + std::time::Duration::from_secs(i as u64 * 3600);
                            let duration = std::time::Duration::from_secs(3600);
                            let _ = scheduler.create_reservation(
                                user.into(),
                                GpuSpec::Count(2),
                                start,
                                duration,
                            );
                        }

                        scheduler
                    },
                    |mut scheduler| {
                        use gflow::core::reservation::GpuSpec;
                        let start =
                            std::time::SystemTime::now() + std::time::Duration::from_secs(7200);
                        let duration = std::time::Duration::from_secs(3600);
                        hint_black_box(scheduler.create_reservation(
                            "testuser".into(),
                            GpuSpec::Count(2),
                            start,
                            duration,
                        ))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark updating reservation statuses
fn bench_update_reservation_statuses(c: &mut Criterion) {
    let mut group = c.benchmark_group("reservation/update_statuses");

    for reservation_count in [10, 50, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("count", reservation_count),
            &reservation_count,
            |b, &reservation_count| {
                b.iter_batched(
                    || {
                        use gflow::core::reservation::GpuSpec;
                        let mut scheduler = create_test_scheduler();
                        let now = std::time::SystemTime::now();

                        // Create reservations with different statuses
                        for i in 0..reservation_count {
                            let user = format!("user{}", i % 10);
                            let offset = (i as i64 - reservation_count as i64 / 2) * 3600;
                            let start = if offset < 0 {
                                now - std::time::Duration::from_secs((-offset) as u64)
                            } else {
                                now + std::time::Duration::from_secs(offset as u64)
                            };
                            let duration = std::time::Duration::from_secs(7200);
                            let _ = scheduler.create_reservation(
                                user.into(),
                                GpuSpec::Count(1),
                                start,
                                duration,
                            );
                        }

                        scheduler
                    },
                    |mut scheduler| {
                        scheduler.update_reservation_statuses();
                        hint_black_box(scheduler.reservations.len())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark scheduling with active reservations
fn bench_scheduling_with_reservations(c: &mut Criterion) {
    let mut group = c.benchmark_group("reservation/scheduling_with_reservations");
    group.sample_size(10);

    for (job_count, reservation_count) in [(10_000, 10), (25_000, 25), (50_000, 50)] {
        group.bench_with_input(
            BenchmarkId::new(
                "jobs_reservations",
                format!("{}j_{}r", job_count, reservation_count),
            ),
            &(job_count, reservation_count),
            |b, &(job_count, reservation_count)| {
                b.iter_batched(
                    || {
                        use gflow::core::reservation::GpuSpec;
                        let mut scheduler = create_test_scheduler();
                        let now = std::time::SystemTime::now();

                        // Create active reservations for different users
                        for i in 0..reservation_count {
                            let user = format!("user{}", i % 10);
                            let start = now - std::time::Duration::from_secs(1800);
                            let duration = std::time::Duration::from_secs(7200);
                            let _ = scheduler.create_reservation(
                                user.into(),
                                GpuSpec::Count(1),
                                start,
                                duration,
                            );
                        }

                        // Create jobs from various users
                        populate_scheduler(&mut scheduler, job_count);

                        scheduler
                    },
                    |mut scheduler| {
                        let jobs = scheduler.prepare_jobs_for_execution();
                        hint_black_box(jobs.len())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    reservation_benches,
    bench_create_reservation,
    bench_update_reservation_statuses,
    bench_scheduling_with_reservations,
);
