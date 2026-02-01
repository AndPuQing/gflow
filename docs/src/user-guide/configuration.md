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
- [GPU Management](./gpu-management) - GPU allocation
