//! Benchmarks for gflow scheduler performance at scale (10k-100k jobs)
//!
//! This benchmark suite measures:
//! - Memory consumption with varying job counts
//! - Query performance (list, filter, lookup)
//! - Job submission throughput
//! - Scheduling decision performance

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
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

/// Benchmark just Job creation (no scheduler, no HashMap)
fn bench_job_creation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/job_creation");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &size, |b, &size| {
            b.iter(|| {
                let jobs: Vec<Job> = (0..size).map(|i| create_test_job(i as u32)).collect();
                hint_black_box(jobs);
            });
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

/// Benchmark submit_job overhead (ID assignment, timestamp, state init)
fn bench_submit_job_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("bottleneck/submit_overhead");
    group.sample_size(10);

    for size in [10_000, 25_000, 50_000, 75_000, 100_000] {
        // Pre-create all jobs
        let jobs: Vec<Job> = (0..size).map(|i| create_test_job(i as u32)).collect();

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("jobs", size), &jobs, |b, jobs| {
            b.iter(|| {
                let mut scheduler = create_test_scheduler();
                for job in jobs.iter() {
                    scheduler.submit_job(job.clone());
                }
                hint_black_box(&scheduler);
            });
        });
    }

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
                    let jobs: Vec<&Job> = scheduler.jobs.iter().collect();
                    std::hint::black_box(jobs.len())
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
        for (i, job) in scheduler.jobs.iter_mut().enumerate() {
            match i % 5 {
                0 => job.state = JobState::Running,
                1 => job.state = JobState::Finished,
                2 => job.state = JobState::Failed,
                3 => job.state = JobState::Hold,
                _ => {} // Keep as Queued
            }
        }

        group.bench_with_input(
            BenchmarkId::new("jobs", size),
            &scheduler,
            |b, scheduler| {
                b.iter(|| {
                    let queued: Vec<&Job> = scheduler
                        .jobs
                        .iter()
                        .filter(|j| j.state == JobState::Queued)
                        .collect();
                    std::hint::black_box(queued.len())
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
                    for (i, job) in scheduler.jobs.iter_mut().enumerate() {
                        if i % 10 == 0 {
                            job.state = JobState::Running;
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
        for (i, job) in scheduler.jobs.iter_mut().enumerate() {
            match i % 5 {
                0 => job.state = JobState::Running,
                1 => job.state = JobState::Finished,
                2 => job.state = JobState::Failed,
                3 => job.state = JobState::Hold,
                _ => {}
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
    bench_job_clone,
    bench_string_allocation,
    bench_submit_job_overhead,
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
);
