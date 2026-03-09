# Benchmark 回归检查

`gflow` 已经在 [`benches/scheduler_bench.rs`](https://github.com/AndPuQing/gflow/blob/main/benches/scheduler_bench.rs) 中维护了一套较完整的 Criterion benchmark。Issue `#129` 的目标，是把其中一小组有代表性的场景变成可以重复执行的性能回归检查。

## 精选 benchmark 集合

这个回归集合刻意保持精简，确保维护者真的会去跑：

| Benchmark | 作用 |
| --- | --- |
| `bottleneck/job_creation/jobs/10000` | 监控调度器插入和队列增长是否变慢。 |
| `query/by_state/index/50000` | 覆盖队列查看时依赖的索引查询路径。 |
| `dependency/validate_circular/jobs/25000` | 保护大规模依赖图的环检测性能。 |
| `group_concurrency/scheduling/jobs/25000` | 覆盖调度中的组并发限制检查。 |
| `scheduling_flow/complete/jobs/25000` | 覆盖 `prepare_jobs_for_execution` 的端到端路径。 |
| `reservation/scheduling_with_reservations/jobs_reservations/25000j_25r` | 防止 reservation 感知调度退化。 |

完整的 Criterion 套件仍然保留，用于深入分析。这里的回归集合只负责提供轻量级安全网。

## 本地使用方式

列出当前回归集合：

```bash
just bench-regression-list
```

创建或刷新本地 baseline：

```bash
just bench-regression-baseline local
```

把当前分支和该 baseline 做比较：

```bash
just bench-regression-compare local
```

每次执行会生成：

- `target/benchmark-regression-summary.json`
- `target/benchmark-regression-summary.md`
- `target/criterion/...`

Criterion 会把已保存 baseline 放在 `target/criterion/<benchmark>/<baseline-name>/` 下，所以后续比较不需要再手工拼参数。

## CI 工作流

仓库新增了一个专用 GitHub Actions workflow：`Benchmark Regression`。

- 通过 `workflow_dispatch` 并选择 `mode=baseline`，会生成名为 `benchmark-baseline-<baseline-name>` 的 baseline artifact。
- 通过 `workflow_dispatch` 并选择 `mode=compare`，会自动下载最新的同名 baseline artifact，再和当前代码比较。
- 定时任务会在每周一执行，同样基于最新 baseline artifact 做比较。

每次 workflow 都会上传：

- 本次运行使用的 Criterion 目录
- JSON 摘要
- 写入到 job summary 的 Markdown 摘要

## Baseline 环境约定

默认共享 baseline 名称为 `ubuntu-22.04-stable`。

它对应的推荐环境是：

- GitHub 托管的 `ubuntu-22.04`
- stable Rust toolchain
- `benches/scheduler_bench.rs` 中的 benchmark harness

如果你在明显不同的环境里刷新 baseline，应该使用不同的 baseline 名称，避免比较结果失真。
