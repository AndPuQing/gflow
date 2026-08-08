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
gflowd start

# Start with restricted GPUs and random allocation
gflowd start --gpus 0,2 --gpu-allocation-strategy random

# Start with faster GPU occupancy polling
gflowd start --gpu-poll-interval-secs 3

# Install the optional systemd user service (auto-start + crash recovery)
gflowd service install

# Reload without downtime
gflowd reload

# Restart the daemon with a new GPU restriction
gflowd restart --gpus 0-3

# Check status or stop the daemon
gflowd status
gflowd stop
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
gflowd init [--yes] [--force] [--advanced] [--gpus <indices>] [--host <host>] [--port <port>] [--timezone <tz>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
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
- `--gpu-poll-interval-secs <seconds>`: poll NVML for GPU occupancy changes every N seconds (default: `10`, minimum: `1`)

### `gflowd start`

Start the daemon. The hosting layer is chosen automatically: the systemd user
service if installed, otherwise tmux, otherwise a direct detached process.

```bash
gflowd start [--gpus <indices>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
```

`up` is retained as an alias of `start`.

### `gflowd reload`

Reload the daemon with zero downtime.

```bash
gflowd reload [--gpus <indices>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
```

Use this when you want to refresh the running daemon without stopping it first.

### `gflowd restart`

Stop the daemon and start it again.

```bash
gflowd restart [--gpus <indices>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
```

Use this when a full restart is acceptable or needed.

### `gflowd status`

Show whether the daemon is running and how it is hosted (systemd user service,
tmux, or direct process).

```bash
gflowd status
```

### `gflowd stop`

Stop the daemon.

```bash
gflowd stop
```

`down` is retained as an alias of `stop`.

### `gflowd service`

Manage the optional systemd user service, which provides auto-start on login
and automatic crash recovery.

```bash
gflowd service install [--gpus <indices>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
gflowd service uninstall
```

`install` writes `~/.config/systemd/user/gflowd.service`, reloads systemd, and
runs `enable --now`. It requires a systemd user manager; on systems without
one it prints a clear message and falls back to tmux/direct hosting.

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
- `--gpu-poll-interval-secs` controls how quickly unmanaged GPU occupancy changes are detected.
- `gflowd start`, `reload`, and `restart` all accept the same GPU-related overrides.
- In direct-process mode (no systemd, no tmux), the daemon holds an exclusive
  `flock` on `gflowd.lock` in the runtime directory. The lock is both mutual
  exclusion (a duplicate `up` is refused) and a crash-safe liveness signal: it
  is released automatically when the daemon exits, so `status` never reports a
  stale instance. The lock file also records the daemon's identity (`pid` +
  `pgid` + process start time); `down`/`restart` verify it before signalling so
  a recycled PID is never SIGTERM/SIGKILLed.

## See Also

- [Configuration](../user-guide/configuration)
- [GPU Management](../user-guide/gpu-management)
- [Quick Reference](./quick-reference)
