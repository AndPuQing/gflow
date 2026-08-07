# gctl Reference

`gctl` changes scheduler behavior at runtime.

## Usage

```bash
gctl <command> [args]
gctl completion <shell>
```

## Commands

### `gctl show-gpus`

Show per-GPU status, including whether a GPU is restricted.

```bash
gctl show-gpus
```

### `gctl gpu-process ignore --gpu <index> --pid <pid>`

Ignore a running GPU process when gflow evaluates whether a GPU is blocked by an unmanaged workload.

This is a runtime-only override. `gflowd` restart or reload clears it automatically.

```bash
gctl gpu-process ignore --gpu 0 --pid 1234
```

### `gctl gpu-process unignore --gpu <index> --pid <pid>`

Remove a runtime GPU-process ignore override.

```bash
gctl gpu-process unignore --gpu 0 --pid 1234
```

### `gctl gpu-process list`

List active runtime GPU-process ignore overrides.

```bash
gctl gpu-process list
```

### `gctl set-gpus <gpu_spec>`

Restrict which GPUs the scheduler can allocate for **new** jobs.

`<gpu_spec>` examples:

- `all`
- `0,2,4`
- `0-3`
- `0-1,3,5-6`

```bash
gctl set-gpus 0,2
gctl set-gpus all
```

### `gctl set-limit <job_or_group_id> <limit>`

Set max concurrency for a job group. To temporarily group independently
submitted jobs, pass a comma-separated list or range of active Job IDs; this
only affects the selected jobs. Selected jobs must be queued, held, or
running and must not already belong to a job group; use the group ID for an
existing group. The limit must be greater than zero.

```bash
gctl set-limit <job_id> 2
gctl set-limit <group_id> 2
gctl set-limit 101,102,103 2
gctl set-limit 201-210 4
```

### `gctl reserve create`

Create a GPU reservation for a specific user.

**By GPU count** (scheduler allocates dynamically):
```bash
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
```

**By specific GPU indices** (reserve exact GPUs):
```bash
gctl reserve create --user alice --gpu-spec 0,2 --start '2026-01-28 14:00' --duration 2h
gctl reserve create --user bob --gpu-spec 0-3 --start '2026-01-28 16:00' --duration 1h
```

`--start` supports ISO8601 (e.g. `2026-01-28T14:00:00Z`) or `YYYY-MM-DD HH:MM` (local time). Times must be on `:00` or `:30`; durations are multiples of 30 minutes.

### `gctl reserve list`

List reservations.

```bash
gctl reserve list
gctl reserve list --active
gctl reserve list --user alice --status active
gctl reserve list --timeline --range 48h
```

### `gctl reserve get <reservation_id>`

Show details for a reservation.

```bash
gctl reserve get <reservation_id>
```

### `gctl reserve cancel <reservation_id>`

Cancel a reservation.

```bash
gctl reserve cancel <reservation_id>
```

### `gctl quota list`

Show quota subjects (users / projects) with effective limits and current
usage (running jobs, running GPUs, queued jobs).

```bash
gctl quota list
```

### `gctl quota set`

Set (merge) runtime quota limits. Select exactly one subject with `--user`,
`--project`, `--default-user` or `--default-project`, and provide at least one
limit flag. Overrides are persisted in the daemon state and take precedence
over the `[quota]` section in `gflow.toml`.

```bash
gctl quota set --user alice --max-running-gpus 4 --max-queued-jobs 50
gctl quota set --project cv-team --max-running-gpus 8
gctl quota set --default-user --max-running-jobs 4
```

### `gctl quota remove`

Remove a runtime quota override so the subject falls back to the `gflow.toml`
baseline.

```bash
gctl quota remove --user alice
gctl quota remove --default-user
```
