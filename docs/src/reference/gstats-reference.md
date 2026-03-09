# gstats Reference

`gstats` shows scheduler usage statistics for a user or time window.

## Usage

```bash
gstats [options]
gstats completion <shell>
```

If no subcommand is given, `gstats` prints statistics immediately.

## Common Examples

```bash
# Current user's all-time stats
gstats

# Last 7 days for the current user
gstats --since 7d

# Another user
gstats --user alice

# All users in JSON
gstats --all-users --output json

# Export flat metrics for scripts
gstats --since today --output csv
```

## Options

- `-u, --user <user>`: filter by one user; default is the current user
- `-a, --all-users`: aggregate across all users
- `-t, --since <when>`: filter by time window such as `1h`, `7d`, `30d`, `today`, or an ISO timestamp
- `-o, --output <format>`: `table`, `json`, or `csv` (default: `table`)

## Output

### Table Output

The default table view includes:

- Job totals and status counts
- Average wait time and runtime
- Total GPU-hours and peak GPU usage
- Success rate
- Top jobs by runtime when available

### JSON Output

`--output json` prints the same statistics as structured JSON.

### CSV Output

`--output csv` prints one `metric,value` row per metric.

Current CSV metrics:

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

Generate shell completion scripts.

```bash
gstats completion bash
gstats completion zsh
gstats completion fish
```

## See Also

- [Quick Reference](./quick-reference)
- [gqueue Reference](./gqueue-reference)
