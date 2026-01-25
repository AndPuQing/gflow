# gctl Reference

`gctl` is a small admin/control CLI for adjusting scheduler behavior at runtime.

## Usage

```bash
gctl <command> [args]
```

## Commands

### `gctl show-gpus`

Show current GPU status and whether each GPU is restricted by the scheduler configuration.

**Output format**:

- `<index>\t<available|in_use>` for allowed GPUs
- `<index>\t<available|in_use>\trestricted` for restricted GPUs

Example:

```bash
gctl show-gpus
```

### `gctl set-gpus <gpu_spec>`

Restrict which GPUs the scheduler can allocate for new jobs.

`<gpu_spec>` can be:

- `all` (remove restriction; allow all detected GPUs)
- A comma-separated list: `0,2,4`
- A range: `0-3`
- A mix: `0-1,3,5-6`

Examples:

```bash
gctl set-gpus 0,2
gctl set-gpus 0-3
gctl set-gpus all
```

Notes:

- Applies to new allocations; running jobs are unaffected.
- Equivalent to configuring `daemon.gpus` in `~/.config/gflow/gflow.toml`.

### `gctl set-limit <job_or_group_id> <limit>`

Set the max concurrency for a job group.

`<job_or_group_id>` can be:

- A job ID (any job in the group); `gctl` will resolve its `group_id`
- A group ID (UUID)

Examples:

```bash
# Using any job ID in the group
gctl set-limit <job_id> 2

# Using a group ID
gctl set-limit <group_id> 2
```

Tip: job groups are created by `gbatch` when submitting parameter batches with `--max-concurrent`.

### `gctl completion <shell>`

Generate shell completion scripts.

```bash
gctl completion bash
gctl completion zsh
gctl completion fish
```
