# gflow v0.4.18 Release Notes

gflow v0.4.18 adds scheduled job starts, a real-time web dashboard, unified
daemon lifecycle management with optional systemd user service, an experimental
process executor, per-user/per-project quotas, and fair-share scheduling.

## Highlights

### Scheduled / delayed job start

Jobs can now be submitted with a wall-clock start time and stay queued (reason
`BeginTime`, Slurm-style) until it arrives, then be released automatically.
`gbatch --begin <time>` defers job initiation (accepts `HH:MM[:SS]`, full
timestamps, or relative `now+N[s|m|h|d]`), and `gjob release <id> --at <time>`
releases a held job so it starts no earlier than the given time. The
`scheduled_at` field persists across daemon restarts, and the begin-time monitor
sleeps precisely until the next start time instead of polling.

### Real-time web dashboard + job log viewer

The web dashboard now refreshes live over a Server-Sent Events stream
(`GET /events`) with automatic fallback to polling when the stream is
unavailable, plus a Live/Polling/Connecting status badge. Each job has a Logs
action that opens a dialog tailing its captured output (`GET
/jobs/{id}/log/content`) with ANSI stripping and follow-on scrolling.

### Unified gflowd lifecycle + optional systemd user service

A consistent `gflowd start` / `stop` / `restart` / `status` verb set hides how
the daemon is hosted. Hosting is chosen by priority — systemd user service →
tmux → direct detached process (pidfile) — and `gflowd status` reports the
current hosting mode plus a daemon summary (version, PID, uptime, executor
backend, GPU availability). New `gflowd service install` / `service uninstall`
manage `~/.config/systemd/user/gflowd.service` with auto-start on login and
`Restart=on-failure` crash recovery. The direct-process path is hardened against
PID reuse with an exclusive `flock` lock and daemon identity verification.

### Process executor (experimental, opt-in)

Jobs can run as detached child process groups (`setsid`) with stdio redirected
to `logs/<job_id>.log` — no tmux required. Opt in via `[executor]
type = "process"` (tmux remains the default). Cancellation SIGTERMs the whole
process group and escalates to SIGKILL after a grace period; running jobs are
recovered across daemon restarts.

### Per-user / per-project quotas

Cap how much of a shared machine one user or project can occupy with the new
`[quota]` config (defaults plus per-name tables), enforced in the scheduling
loop (`max_running_jobs`, `max_running_gpus`) and at submission time
(`max_queued_jobs`). Managed at runtime via `gctl quota list` / `set` / `remove`
and persisted in daemon state.

### Fair-share scheduling

Queued jobs are reordered so users with less recent GPU-time usage are
scheduled first, using Slurm-style exponentially decayed per-user GPU-time
accounting (`gpus × runtime`) that persists across restarts. Reordering stays
within the same priority band and never overrides group limits, reservations,
or resource availability. Configurable via `[daemon.fair_share]`.

## Compatibility

v0.4.18 is intended as a drop-in patch upgrade from v0.4.17. It does not change
CLI commands, HTTP endpoints, MCP tools, or configuration formats, with two
additions: the `COMMAND` field is optional in `gqueue -f` output (default output
unchanged), and the process executor is opt-in. Existing scheduler state is
loaded through the backward-compatible reader.

After the release is published, upgrade the PyPI package with:

```bash
python -m pip install --upgrade runqd==0.4.18
```

## Known Limitations

- The process executor is experimental and not yet considered stable; `gjob
  attach` / `gjob close-sessions` continue to work in the default tmux mode.
- The web dashboard's log viewer tails the captured job log; it does not stream
  terminal session output interactively.
