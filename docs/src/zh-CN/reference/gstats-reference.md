# gstats 参考

`gstats` 用于查看指定用户或时间窗口内的调度器使用统计信息。

## 用法

```bash
gstats [options]
gstats completion <shell>
```

如果不带子命令，`gstats` 会直接输出统计结果。

## 常见示例

```bash
# 当前用户的全量统计
gstats

# 当前用户最近 7 天
gstats --since 7d

# 指定用户
gstats --user alice

# 所有用户，JSON 输出
gstats --all-users --output json

# 导出适合脚本处理的扁平指标
gstats --since today --output csv
```

## 选项

- `-u, --user <user>`：指定单个用户；默认是当前用户
- `-a, --all-users`：汇总所有用户
- `-t, --since <when>`：按时间窗口筛选，例如 `1h`、`7d`、`30d`、`today` 或 ISO 时间戳
- `-o, --output <format>`：`table`、`json` 或 `csv`，默认 `table`

## 输出格式

### 表格输出

默认表格会包含：

- 任务总数和各状态数量
- 平均等待时间与运行时间
- 总 GPU 小时数与峰值 GPU 使用量
- 成功率
- 若存在，按运行时长排序的 Top 任务

### JSON 输出

`--output json` 会以结构化 JSON 输出同一组统计信息。

### CSV 输出

`--output csv` 会按 `metric,value` 的格式输出，每行一个指标。

当前 CSV 指标包括：

- `total_jobs`
- `completed_jobs`
- `failed_jobs`
- `cancelled_jobs`
- `timeout_jobs`
- `running_jobs`
- `queued_jobs`
- `avg_wait_secs`
- `avg_runtime_secs`
- `total_gpu_hours`
- `jobs_with_gpus`
- `avg_gpus_per_job`
- `peak_gpu_usage`
- `success_rate`

### `gstats completion <shell>`

生成 shell 自动补全脚本。

```bash
gstats completion bash
gstats completion zsh
gstats completion fish
```

## 另见

- [快速参考](./quick-reference)
- [gqueue 参考](./gqueue-reference)
