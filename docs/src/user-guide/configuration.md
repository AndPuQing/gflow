# Configuration

Most users can run gflow without configuration. Use a config file (TOML) and/or environment variables when you need to change where the daemon listens, or restrict GPU usage.

## Config File

Default location:

```
~/.config/gflow/gflow.toml
```

Minimal example:

```toml
[daemon]
host = "localhost"
port = 59000
# gpus = [0, 2]
```

All CLIs accept `--config <path>` to use a different file:

```bash
gflowd --config <path> up
ginfo --config <path>
gbatch --config <path> --gpus 1 python train.py
```

## Daemon Settings

### Host and Port

```toml
[daemon]
host = "localhost"
port = 59000
```

- Default: `localhost:59000`
- Use `0.0.0.0` only if you understand the security implications.

<a id="gpu-selection"></a>

#### GPU Selection

Restrict which physical GPUs the scheduler is allowed to allocate.

Config file:

```toml
[daemon]
gpus = [0, 2]
```

Daemon CLI flag (overrides config):

```bash
gflowd up --gpus 0,2
gflowd restart --gpus 0-3
```

Runtime control (affects new allocations only):

```bash
gctl set-gpus 0,2
gctl set-gpus all
gctl show-gpus
```

Supported specs: `0`, `0,2,4`, `0-3`, `0-1,3,5-6`.

Precedence (highest → lowest):
1. CLI flag (`gflowd up --gpus ...`)
2. Env var (`GFLOW_DAEMON_GPUS=...`)
3. Config file (`daemon.gpus = [...]`)
4. Default: all detected GPUs

### Logging

- `gflowd`: use `-v/--verbose` (see `gflowd --help`).
- Client commands (`gbatch`, `gqueue`, `ginfo`, `gjob`, `gctl`): use `RUST_LOG` (e.g. `RUST_LOG=info`).

## Environment Variables

```bash
export GFLOW_DAEMON_HOST=localhost
export GFLOW_DAEMON_PORT=59000
export GFLOW_DAEMON_GPUS=0,2
```

## Files and State

gflow follows the XDG Base Directory spec:

```text
~/.config/gflow/gflow.toml
~/.local/share/gflow/state.json
~/.local/share/gflow/logs/<job_id>.log
```

## Troubleshooting

### Config file not found

```bash
ls -la ~/.config/gflow/gflow.toml
```

### Port already in use

Change the port:

```toml
[daemon]
port = 59001
```

## See Also

- [Installation](../getting-started/installation) - Initial setup
- [Quick Start](../getting-started/quick-start) - Basic usage
- [GPU Management](./gpu-management) - GPU allocation
