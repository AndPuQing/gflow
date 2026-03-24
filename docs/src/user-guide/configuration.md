# Configuration

Most users can run gflow without configuration. Use a config file (TOML) and/or environment variables when you need to change where the daemon listens, or restrict GPU usage.

## Config File

Default location:

```
~/.config/gflow/gflow.toml
```

Generate one interactively:

```bash
gflowd init
```

Minimal example:

```toml
[daemon]
host = "localhost"
port = 59000
# gpus = [0, 2]
# gpu_allocation_strategy = "sequential" # or "random"
# gpu_poll_interval_secs = 10
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

#### GPU Allocation Strategy

Control how gflow picks GPU indices when multiple GPUs are available.

Config file:

```toml
[daemon]
gpu_allocation_strategy = "sequential" # default
# gpu_allocation_strategy = "random"
```

- `sequential`: deterministic, prefer lower GPU indices first.
- `random`: randomize GPU selection order each scheduling cycle.

Daemon CLI flag (overrides config):

```bash
gflowd up --gpu-allocation-strategy random
gflowd restart --gpu-allocation-strategy sequential
```

Daemon CLI flag (overrides config):

```bash
gflowd up --gpus 0,2
gflowd restart --gpus 0-3
```

#### GPU Poll Interval

Control how quickly gflow notices unmanaged GPU occupancy changes.

Config file:

```toml
[daemon]
gpu_poll_interval_secs = 3 # default: 10
```

- Lower values react faster to external GPU usage changes, but poll NVML more often.
- Value must be at least `1`.

Daemon CLI flag (overrides config):

```bash
gflowd up --gpu-poll-interval-secs 3
gflowd reload --gpu-poll-interval-secs 1
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
2. Env var (`GFLOW_DAEMON__GPUS=...`)
3. Config file (`daemon.gpus = [...]`)
4. Default: all detected GPUs

For allocation strategy:
1. CLI flag (`gflowd up --gpu-allocation-strategy ...`)
2. Env var (`GFLOW_DAEMON__GPU_ALLOCATION_STRATEGY=...`)
3. Config file (`daemon.gpu_allocation_strategy = "..."`)
4. Default: `sequential`

For GPU poll interval:
1. CLI flag (`gflowd up --gpu-poll-interval-secs ...`)
2. Env var (`GFLOW_DAEMON__GPU_POLL_INTERVAL_SECS=...`)
3. Config file (`daemon.gpu_poll_interval_secs = ...`)
4. Default: `10`

## Timezone

Configure timezone for displaying and parsing reservation times.

Config file:

```toml
timezone = "Asia/Shanghai"
```

Per-command override:

```bash
gctl reserve create --user alice --gpus 2 --start "2026-02-01 14:00" --duration "2h" --timezone "UTC"
```

Supported formats:
- IANA timezone names: `"Asia/Shanghai"`, `"America/Los_Angeles"`, `"UTC"`
- Time input: ISO8601 (`"2026-02-01T14:00:00Z"`) or simple format (`"2026-02-01 14:00"`)

Precedence (highest → lowest):
1. CLI flag (`--timezone`)
2. Config file (`timezone = "..."`)
3. Default: local system timezone

## Project Tracking

Use project settings to standardize job ownership metadata across teams.

```toml
[projects]
known_projects = ["ml-research", "cv-team"]
require_project = false
```

- `known_projects`: allowed project codes. Empty means any non-empty code is allowed.
- `require_project`: when `true`, every submitted job must include a non-empty project.
- Project values are normalized (trimmed). Whitespace-only values are treated as unset.
- Project code length limit: 64 characters.
- If both settings are used, project must be present and in `known_projects`.

Related CLI usage:

```bash
gbatch --project ml-research python train.py
gqueue --project ml-research
gqueue --format JOBID,NAME,PROJECT,ST,TIME
```

## Notifications

Use [Notifications](./notifications) when you need webhook or email delivery for job and system events.

- `notifications.emails` is also the SMTP transport used by per-job flags such as `gbatch --notify-email`.
- Keep the daemon on `localhost` when possible if notification payloads contain sensitive job metadata.

### Logging

- `gflowd`: use `-v/--verbose` (see `gflowd --help`).
- Client commands (`gbatch`, `gqueue`, `ginfo`, `gjob`, `gctl`): use `RUST_LOG` (e.g. `RUST_LOG=info`).

## Environment Variables

Nested daemon keys use double underscores (`__`).

```bash
export GFLOW_DAEMON__HOST=localhost
export GFLOW_DAEMON__PORT=59000
export GFLOW_DAEMON__GPUS=0,2
export GFLOW_DAEMON__GPU_ALLOCATION_STRATEGY=random
export GFLOW_DAEMON__GPU_POLL_INTERVAL_SECS=3
```

## Files and State

gflow follows the XDG Base Directory spec:

```text
~/.config/gflow/gflow.toml
~/.local/share/gflow/state.msgpack  (or state.json for legacy)
~/.local/share/gflow/logs/<job_id>.log
```

### State Persistence Format

Starting from version 0.4.11, gflowd uses **MessagePack** binary format for state persistence:

- **New installations**: State is saved to `state.msgpack` (binary format)
- **Automatic migration**: Existing `state.json` files are automatically migrated to `state.msgpack` on first load
- **Backward compatibility**: gflowd can still read old `state.json` files

### Recovery mode (state file issues)

If the state file cannot be deserialized or migrated (e.g. after upgrading/downgrading versions), `gflowd` enters **recovery mode**:

- `gflowd` continues running, but does not overwrite the state file.
- State changes are persisted to a single-snapshot journal file: `~/.local/share/gflow/state.journal.jsonl` (it is overwritten on each save).
- `/health` returns `200` with `status: "recovery"` and `mode: "journal"`.
- A backup copy is created next to the state file (e.g. `state.msgpack.backup.<timestamp>` or `state.msgpack.corrupt.<timestamp>`).

When the state file becomes readable again, `gflowd` loads the latest journal snapshot, rewrites the state file, and truncates the journal.

If the journal file is not writable, `gflowd` falls back to **read-only** mode and mutating APIs return `503`.

To recover, upgrade/downgrade to a version that can read/migrate your state, or restore from the backup file.

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
- [Multi-User Usage](./multi-user) - Shared scheduler setup
- [GPU Management](./gpu-management) - GPU allocation
