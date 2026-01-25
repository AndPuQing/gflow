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

