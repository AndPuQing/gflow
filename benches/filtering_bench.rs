use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use gflow::core::job::{Job, JobBuilder, JobState};
use std::hint::black_box;
use std::time::{Duration, SystemTime};

/// Generate test jobs with varying states and users
fn generate_jobs(count: usize) -> Vec<Job> {
    let states = [
        JobState::Queued,
        JobState::Running,
        JobState::Finished,
        JobState::Failed,
        JobState::Cancelled,
    ];
    let users = ["alice", "bob", "charlie", "david"];

    (0..count)
        .map(|i| {
            let mut job = JobBuilder::new()
                .submitted_by(users[i % users.len()].to_string())
                .run_dir("/tmp")
                .build();
            // Manually set the state for testing
            job.state = states[i % states.len()];
            job
        })
        .collect()
}

/// Old implementation: multiple passes with retain
fn filter_jobs_old(
    jobs: Vec<Job>,
    state_filter: Option<Vec<JobState>>,
    user_filter: Option<Vec<String>>,
    time_filter: Option<SystemTime>,
) -> Vec<Job> {
    let mut jobs = jobs;

    // Apply state filter
    if let Some(states) = state_filter {
        if !states.is_empty() {
            jobs.retain(|job| states.contains(&job.state));
        }
    }

    // Apply user filter
    if let Some(users) = user_filter {
        if !users.is_empty() {
            jobs.retain(|job| users.iter().any(|u| u == job.submitted_by.as_str()));
        }
    }

    // Apply time filter
    if let Some(created_after) = time_filter {
        jobs.retain(|job| job.submitted_at.is_some_and(|ts| ts >= created_after));
    }

    // Sort by job ID
    jobs.sort_by_key(|j| j.id);

    jobs
}

/// New implementation: single pass with filter
fn filter_jobs_new(
    jobs: &[Job],
    state_filter: Option<Vec<JobState>>,
    user_filter: Option<Vec<String>>,
    time_filter: Option<SystemTime>,
) -> Vec<Job> {
    let mut jobs: Vec<_> = jobs
        .iter()
        .filter(|job| {
            // Apply state filter
            if let Some(ref states) = state_filter {
                if !states.is_empty() && !states.contains(&job.state) {
                    return false;
                }
            }

            // Apply user filter
            if let Some(ref users) = user_filter {
                if !users.is_empty() && !users.iter().any(|u| u == job.submitted_by.as_str()) {
                    return false;
                }
            }

            // Apply time filter
            if let Some(created_after) = time_filter {
                if job.submitted_at.is_none_or(|ts| ts < created_after) {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect();

    // Sort by job ID
    jobs.sort_unstable_by_key(|j| j.id);

    jobs
}

fn benchmark_filtering(c: &mut Criterion) {
    let mut group = c.benchmark_group("job_filtering");

    // Test with different job counts
    for count in [100, 1000, 10000] {
        let jobs = generate_jobs(count);

        // Prepare filters
        let state_filter = Some(vec![JobState::Running, JobState::Queued]);
        let user_filter = Some(vec!["alice".to_string(), "bob".to_string()]);
        let time_filter = Some(SystemTime::now() - Duration::from_secs(3600));

        // Benchmark old implementation
        group.bench_with_input(BenchmarkId::new("old_multi_pass", count), &count, |b, _| {
            b.iter(|| {
                filter_jobs_old(
                    black_box(jobs.clone()),
                    black_box(state_filter.clone()),
                    black_box(user_filter.clone()),
                    black_box(time_filter),
                )
            });
        });

        // Benchmark new implementation
        group.bench_with_input(
            BenchmarkId::new("new_single_pass", count),
            &count,
            |b, _| {
                b.iter(|| {
                    filter_jobs_new(
                        black_box(&jobs),
                        black_box(state_filter.clone()),
                        black_box(user_filter.clone()),
                        black_box(time_filter),
                    )
                });
            },
        );
    }

    group.finish();
}

fn benchmark_no_filters(c: &mut Criterion) {
    let mut group = c.benchmark_group("job_filtering_no_filters");

    for count in [100, 1000, 10000] {
        let jobs = generate_jobs(count);

        // Benchmark old implementation with no filters
        group.bench_with_input(BenchmarkId::new("old_multi_pass", count), &count, |b, _| {
            b.iter(|| {
                filter_jobs_old(
                    black_box(jobs.clone()),
                    black_box(None),
                    black_box(None),
                    black_box(None),
                )
            });
        });

        // Benchmark new implementation with no filters
        group.bench_with_input(
            BenchmarkId::new("new_single_pass", count),
            &count,
            |b, _| {
                b.iter(|| {
                    filter_jobs_new(
                        black_box(&jobs),
                        black_box(None),
                        black_box(None),
                        black_box(None),
                    )
                });
            },
        );
    }

    group.finish();
}

fn benchmark_single_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("job_filtering_single_filter");

    for count in [100, 1000, 10000] {
        let jobs = generate_jobs(count);
        let state_filter = Some(vec![JobState::Running]);

        // Benchmark old implementation with single filter
        group.bench_with_input(BenchmarkId::new("old_multi_pass", count), &count, |b, _| {
            b.iter(|| {
                filter_jobs_old(
                    black_box(jobs.clone()),
                    black_box(state_filter.clone()),
                    black_box(None),
                    black_box(None),
                )
            });
        });

        // Benchmark new implementation with single filter
        group.bench_with_input(
            BenchmarkId::new("new_single_pass", count),
            &count,
            |b, _| {
                b.iter(|| {
                    filter_jobs_new(
                        black_box(&jobs),
                        black_box(state_filter.clone()),
                        black_box(None),
                        black_box(None),
                    )
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_filtering,
    benchmark_no_filters,
    benchmark_single_filter
);
criterion_main!(benches);
