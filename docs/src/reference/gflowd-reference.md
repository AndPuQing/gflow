# gflowd Reference

`gflowd` manages the local gflow daemon.

## Usage

```bash
gflowd [options] [command]
gflowd completion <shell>
```

## Common Examples

```bash
# Initialize config interactively
gflowd init

# Initialize config non-interactively with defaults
gflowd init --yes

# Start the daemon
gflowd up

# Start with restricted GPUs and random allocation
gflowd up --gpus 0,2 --gpu-allocation-strategy random

# Reload without downtime
gflowd reload

# Restart the daemon with a new GPU restriction
gflowd restart --gpus 0-3

# Check status or stop the daemon
gflowd status
gflowd down
```

## Global Options

- `-c, --config <path>`: use a custom config file
- `--cleanup`: clean up the configuration file
- `-v/-vv/-vvv/-vvvv`: increase daemon logging verbosity
- `-q`: reduce daemon logging verbosity

## Commands

### `gflowd init`

Create or update the configuration file via a guided wizard.

```bash
gflowd init [--yes] [--force] [--advanced] [--gpus <indices>] [--host <host>] [--port <port>] [--timezone <tz>] [--gpu-allocation-strategy <strategy>]
```

Options:

- `--yes`: accept all defaults without prompts
- `--force`: overwrite an existing config file
- `--advanced`: configure advanced options such as notifications
- `--gpus <indices>`: restrict scheduler-visible GPUs, for example `0,2` or `0-2`
- `--host <host>`: daemon host (default: `localhost`)
- `--port <port>`: daemon port (default: `59000`)
- `--timezone <tz>`: store a timezone like `Asia/Shanghai` or `UTC`; use `local` to leave it unset
- `--gpu-allocation-strategy <strategy>`: `sequential` or `random`

### `gflowd up`

Start the daemon in a tmux session.

```bash
gflowd up [--gpus <indices>] [--gpu-allocation-strategy <strategy>]
```

### `gflowd reload`

Reload the daemon with zero downtime.

```bash
gflowd reload [--gpus <indices>] [--gpu-allocation-strategy <strategy>]
```

Use this when you want to refresh the running daemon without stopping it first.

### `gflowd restart`

Stop the daemon and start it again.

```bash
gflowd restart [--gpus <indices>] [--gpu-allocation-strategy <strategy>]
```

Use this when a full restart is acceptable or needed.

### `gflowd status`

Show whether the daemon is running and responding to health checks.

```bash
gflowd status
```

### `gflowd down`

Stop the daemon.

```bash
gflowd down
```

### `gflowd completion <shell>`

Generate shell completion scripts.

```bash
gflowd completion bash
gflowd completion zsh
gflowd completion fish
```

## Notes

- `--gpus` affects which GPUs the scheduler may allocate for new work.
- `--gpu-allocation-strategy` accepts `sequential` or `random`.
- `gflowd up`, `reload`, and `restart` all accept the same GPU-related overrides.

## See Also

- [Configuration](../user-guide/configuration)
- [GPU Management](../user-guide/gpu-management)
- [Quick Reference](./quick-reference)
