use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use gflow::core::job::{Job, JobState};
use gflow::core::scheduler::{Scheduler, SchedulerBuilder};
use std::hint::black_box;
use std::time::SystemTime;

fn create_test_scheduler(num_jobs: usize) -> Scheduler {
    let mut scheduler = SchedulerBuilder::new().build();

    for i in 0..num_jobs {
        let job = Job {
            id: 0, // Will be assigned by submit_job
            script: Some(format!("/path/to/script_{}.sh", i).into()),
            command: Some(format!(
                "echo 'Running job {}' && sleep 1 && echo 'Done'",
                i
            )),
            gpus: i as u32 % 4,
            conda_env: Some(format!("env_{}", i % 3).into()),
            run_dir: format!("/home/user/jobs/job_{}", i).into(),
            priority: (i % 10) as u8,
            depends_on: if i > 0 { Some(i as u32) } else { None },
            depends_on_ids: vec![].into(),
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: Some(i as u32),
            time_limit: Some(std::time::Duration::from_secs(3600)),
            memory_limit_mb: Some(8192),
            submitted_by: format!("user_{}", i % 5).into(),
            redone_from: None,
            auto_close_tmux: false,
            parameters: [
                ("param1".to_string(), format!("value_{}", i)),
                ("param2".to_string(), "some_value".to_string()),
                ("param3".to_string(), format!("data_{}", i * 2)),
            ]
            .into_iter()
            .collect(),
            group_id: Some(format!("group_{}", i % 10)),
            max_concurrent: Some(5),
            run_name: Some(format!("job_{}", i).into()),
            state: match i % 5 {
                0 => JobState::Queued,
                1 => JobState::Running,
                2 => JobState::Finished,
                3 => JobState::Failed,
                _ => JobState::Hold,
            },
            gpu_ids: if i % 4 > 0 {
                Some(vec![i as u32 % 4])
            } else {
                None
            },
            submitted_at: Some(SystemTime::now()),
            started_at: if i % 5 == 1 {
                Some(SystemTime::now())
            } else {
                None
            },
            finished_at: if i % 5 == 2 {
                Some(SystemTime::now())
            } else {
                None
            },
            reason: None,
        };
        scheduler.submit_job(job);
    }

    scheduler
}

fn bench_json_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialize_json");

    for size in [10, 100, 1000].iter() {
        let scheduler = create_test_scheduler(*size);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| serde_json::to_string(black_box(&scheduler)).unwrap());
        });
    }

    group.finish();
}

fn bench_msgpack_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialize_msgpack");

    for size in [10, 100, 1000].iter() {
        let scheduler = create_test_scheduler(*size);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| rmp_serde::to_vec(black_box(&scheduler)).unwrap());
        });
    }

    group.finish();
}

fn bench_json_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize_json");

    for size in [10, 100, 1000].iter() {
        let scheduler = create_test_scheduler(*size);
        let json = serde_json::to_string(&scheduler).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| serde_json::from_str::<Scheduler>(black_box(&json)).unwrap());
        });
    }

    group.finish();
}

fn bench_msgpack_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize_msgpack");

    for size in [10, 100, 1000].iter() {
        let scheduler = create_test_scheduler(*size);
        let msgpack = rmp_serde::to_vec(&scheduler).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| rmp_serde::from_slice::<Scheduler>(black_box(&msgpack)).unwrap());
        });
    }

    group.finish();
}

fn bench_size_comparison(c: &mut Criterion) {
    let group = c.benchmark_group("size_comparison");

    for size in [10, 100, 1000].iter() {
        let scheduler = create_test_scheduler(*size);

        let json = serde_json::to_string(&scheduler).unwrap();
        let msgpack = rmp_serde::to_vec(&scheduler).unwrap();

        println!("\n{} jobs:", size);
        println!("  JSON size:       {} bytes", json.len());
        println!("  MessagePack size: {} bytes", msgpack.len());
        println!(
            "  Reduction:       {:.1}%",
            (1.0 - msgpack.len() as f64 / json.len() as f64) * 100.0
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_json_serialize,
    bench_msgpack_serialize,
    bench_json_deserialize,
    bench_msgpack_deserialize,
    bench_size_comparison
);
criterion_main!(benches);
