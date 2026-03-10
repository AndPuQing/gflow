# Benchmark Regression

`gflow` already ships a broad Criterion benchmark suite in [`benches/scheduler_bench.rs`](https://github.com/AndPuQing/gflow/blob/main/benches/scheduler_bench.rs). Issue `#129` turns a small, representative subset of that suite into a repeatable regression check for scheduler hot paths.

## Curated benchmark set

The regression suite is intentionally small so maintainers will actually run it:

| Benchmark | Why it is included |
| --- | --- |
| `bottleneck/job_creation/jobs/10000` | Detects slower scheduler insertion and queue growth. |
| `query/by_state/index/50000` | Covers indexed queue lookups used by queue inspection. |
| `dependency/validate_circular/jobs/25000` | Protects large dependency graph validation. |
| `group_concurrency/scheduling/jobs/25000` | Exercises group concurrency checks in scheduling. |
| `scheduling_flow/complete/jobs/25000` | Covers the end-to-end `prepare_jobs_for_execution` path. |
| `reservation/scheduling_with_reservations/jobs_reservations/25000j_25r` | Keeps reservation-aware scheduling from regressing. |

The full Criterion suite is still available for deeper investigations. The regression suite is only the lightweight safety net.

## Local workflow

List the curated cases:

```bash
just bench-regression-list
```

Create or refresh a local baseline:

```bash
just bench-regression-baseline local
```

Compare the current branch to that baseline:

```bash
just bench-regression-compare local
```

Each run writes:

- `target/benchmark-regression-summary.json`
- `target/benchmark-regression-summary.md`
- `target/criterion/...`

Criterion stores the saved baseline inside `target/criterion/<benchmark>/<baseline-name>/`, so repeated comparisons do not need ad hoc command arguments.

## CI workflow

The repository includes a dedicated GitHub Actions workflow: `Benchmark Regression`.

- `workflow_dispatch` with `mode=baseline` captures a fresh baseline artifact named `benchmark-baseline-<baseline-name>`.
- `workflow_dispatch` with `mode=compare` downloads the latest matching baseline artifact and compares the current code against it.
- The scheduled run executes every Monday and also compares against the latest baseline artifact.

Each workflow run uploads:

- the Criterion directory used for the run
- the JSON summary
- the Markdown summary added to the job summary

## Baseline environment

The default shared baseline name is `ubuntu-22.04-stable`.

That baseline is intended to be refreshed from:

- GitHub-hosted `ubuntu-22.04`
- the stable Rust toolchain
- the benchmark harness in `benches/scheduler_bench.rs`

If you refresh the baseline in a materially different environment, use a different baseline name so comparisons stay meaningful.
