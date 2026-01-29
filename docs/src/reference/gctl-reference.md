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

Set max concurrency for a job group.

```bash
gctl set-limit <job_id> 2
gctl set-limit <group_id> 2
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
