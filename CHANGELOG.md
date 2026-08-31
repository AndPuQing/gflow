# Changelog

All notable changes to gflow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **web: dark mode with toggle**: the console follows the system color
  scheme by default and adds a light/dark toggle in the header; the choice
  persists in `localStorage` and is applied before first paint to avoid a
  flash. Reservation states (Pending/Active/Completed) now have matching
  status-badge colors.
- **web: click a job row to open its log dialog**, in addition to the per-row
  log button.

### Changed
- **docs: restyle the landing page (runqd.com) into an IBM Carbon / Swiss
  industrial style**: the homepage is a flat, grid-driven layout built only
  from 1px hairlines — no rounded cards, shadows, gradients, or decorative
  elements — in pure white / deep charcoal with a single IBM Blue `#0f62fe`
  accent. Typography moves to IBM Plex Sans (display + body) and IBM Plex Mono
  (labels, code, specs), with a large bold grotesque hero, numbered section
  rows (01–05), a flat borderless terminal, a dark MCP band, and a solid IBM
  Blue closing band. Verified flat and clean in light, dark, zh-CN, and mobile
  via headless screenshots.
- **web: jobs table shows runtime instead of "Starts at"**: the almost-empty
  scheduled-time column is replaced by a `Runtime` column — elapsed time for
  running jobs (grows with refreshes) and total runtime for finished ones.
- **web: load older jobs**: the console starts with the latest 100 jobs and
  gains a "Load earlier jobs" button that pages back through history; the
  header badge now reads `N jobs · showing M` so the page limit is not
  mistaken for the total.
- **web: calmer overview cards**: metric cards use a neutral background with
  the accent color limited to the value and icon, replacing the full pastel
  card tints.
- **web: redesign the GPU cell in the jobs table**: the old pill had
  misaligned 10px segment text; it is now a fixed-height chip row that
  aligns with the row text — an `N×` count, one emerald chip per assigned
  GPU index, and a single dashed chip summarising pending GPUs (`+N`);
  jobs without GPUs show a muted dash instead of a pill.
- **web: sort the jobs table via clickable column headers** with an active
  direction indicator, replacing the sort-field and sort-direction dropdowns;
  the filter row is shorter as a result.
- **web: simplify the GPU view**: the per-slot capsule strip is replaced by
  status cards (status dot + badge, readable busy reason, UUID); terse
  daemon reasons like `manual_ignore(gpu=1,pid=...)` are rendered as
  human-readable text, and blocked slots get their own style/badge.
- **web: fix branding**: the header now says `gflow` (previously `runqd`) and
  the page title is `gflow Dashboard` (previously `web`).
- **web: loading skeleton mirrors the real layout** (4 overview cards plus
  the main panel) instead of a generic 8-card grid.
- **web: remove redundant pieces**: the unused `Separator` UI component, the
  `SelectControl` duplicate (replaced by a shared `Select` in `components/ui`),
  the unused `ApiResult` type, and unused placeholder assets.
- **gqueue: column width modifiers in `-f/--format`**: any field accepts a
  `:WIDTH` suffix (`COMMAND:60` caps the column with a `…`; `:0` or `:full`
  shows it in full). Explicit widths also apply when piped and disable the
  automatic terminal-width fitting.

### Changed
- **web: upgrade `@tanstack/react-table` to v9**: adapt the Jobs view to the
  v9 API — register features/row models via `tableFeatures`, switch
  `useReactTable` to `useTable`, and use the feature-aware `ColumnDef`/
  `Row`/`Table` types. The Jobs table now registers only the features it
  uses (column, global, and row sorting).

### Fixed
- **web: job log dialog fixes**: the dialog actually renders wide now (the
  default `sm:max-w-lg` was overriding the intended width, so logs showed in
  a 512px box); tail auto-follow is fixed — the view pins to the bottom on
  load and refresh, with a "Latest" button to jump back after scrolling up;
  raw tmux-capture logs are cleaned more completely (CSI sequences with
  private markers, OSC, charset designators, stray control chars, and CR
  runs); the log area is taller (55vh) and wraps without breaking words
  mid-token, plus a copy button; finished jobs stop polling.
- **CI: fix `clippy::needless-bool` in reservation filtering**: simplify the
  active-only reservation predicate so the nightly smoke check passes with
  warnings denied.
- **gqueue: long fields no longer break the table layout**: on a terminal,
  tables are truncated to fit the terminal width (widest columns first,
  `…` suffix), so long `COMMAND` values stay on one line; piped or
  redirected output keeps full content. Applies to table, `-g`, and `-t`
  views.

## [0.4.18] - 2026-08-25

### Added
- **Real-time web dashboard + job log viewer**: the dashboard now refreshes
  live over a Server-Sent Events stream (`GET /events`) instead of requiring
  manual reloads, with automatic fallback to polling when the stream is
  unavailable and a Live/Polling/Connecting status badge. Each job has a Logs
  action that opens a dialog tailing its captured output (`GET
  /jobs/{id}/log/content`) with ANSI stripping and follow-on scrolling (#176).
- **`COMMAND` field for `gqueue -f`**: `gqueue -a -f JOBID,NAME,ST,COMMAND` now shows what each job runs — the stored command for command submissions, the script path for script submissions (script wins when both are present, matching the executors; `-` when neither). Optional field; default output unchanged (#192).
- **Scheduled / delayed job start (`--begin` and delayed release)**: jobs can now be submitted with a wall-clock start time and stay queued (reason `BeginTime`, Slurm-style) until it arrives, then be released automatically
  - `gbatch --begin <time>` defers job initiation; accepts `HH:MM[:SS]` (next occurrence), `YYYY-MM-DD[THH:MM[:SS]]`, or relative `now+N[s|m|h|d]` (minutes by default); also supported via `# GFLOW --begin=...` script directives
  - `gjob release <id> --at <time>` releases a held job so it starts no earlier than the given time (delayed release)
  - New `scheduled_at` field on the job model persists across daemon restarts; a begin-time monitor sleeps precisely until the next start time (no polling, woken early by events) and releases due jobs with an immediate scheduling pass; the Web Dashboard shows a "Starts at" column
- **`gflowd status` daemon summary**: when the daemon is running, `gflowd status`
  now also prints a daemon summary fetched from `GET /status` — version, PID,
  uptime, job executor backend, and GPU availability (total/available) — on top
  of the existing hosting mode, for all hosting paths (systemd, tmux, direct).
- **Unified gflowd lifecycle + optional systemd user service**: a consistent `gflowd start` / `stop` / `restart` / `status` verb set that hides how the daemon is hosted from the user
  - Hosting is chosen automatically by priority: systemd user service → tmux → direct detached process (pidfile); `status` reports the current hosting mode
  - New `gflowd service install` / `gflowd service uninstall` manage `~/.config/systemd/user/gflowd.service` (`enable --now`, auto-start on login + `Restart=on-failure` crash recovery); `ExecStart` reuses the existing `daemon_start_args` so the daemon core is unchanged
  - `up` / `down` are retained as aliases of `start` / `stop` for backwards compatibility
- **Process executor (experimental, opt-in)**: jobs can run as detached child process groups (`setsid`) with stdio redirected to `logs/<job_id>.log` — no tmux required for job execution
  - New `[executor] type = "process" | "tmux"` config; `tmux` is the default, the process executor is opt-in and not yet considered stable (`gjob attach` / `gjob close-sessions` keep working in tmux mode)
  - Cancellation SIGTERMs the whole process group and escalates to SIGKILL after a grace period; zombie detection uses real process liveness instead of tmux-session existence
  - `generate_wrapped_command` key-escaping (`\`, `"`, `$`, backtick) is only kept for tmux mode; the process path spawns `bash -c` directly
  - Running jobs are recovered across daemon restarts: the daemon rebuilds the executor state from the journal and re-links surviving process groups
  - `gflowd up` falls back to hosting the daemon as a detached process (pidfile-tracked) when tmux is unavailable; `gflowd down` / `gflowd status` handle both modes
  - `gqueue`'s job-name liveness indicator is executor-aware: tmux mode keeps the session-alive ○, process mode shows ○ from a daemon-reported per-job liveness hint (new `alive` field on the jobs API; daemon `/info` advertises the executor backend)
- **Per-User / Per-Project Resource Quotas**: cap how much of a shared machine one user or project can occupy
  - New `[quota]` config with `default_user` / `default_project` fallbacks plus per-name `[quota.users]` / `[quota.projects]` tables; named entries merge over defaults field-by-field
  - Limits: `max_running_jobs` and `max_running_gpus` (enforced in the scheduling loop — over-limit jobs stay queued with reason `Quota`) and `max_queued_jobs` (enforced at submission — over-limit submissions are rejected)
  - A job must satisfy both its user quota and its project quota; jobs without a project have no project quota
  - Runtime management: `gctl quota list` / `gctl quota set` / `gctl quota remove`, backed by new `GET/PUT/DELETE /quotas` HTTP endpoints; overrides persist in daemon state and take precedence over `gflow.toml`

- **Fair-Share Scheduling**: reorder queued jobs so users with less recent GPU-time usage are scheduled first
  - Slurm-style exponentially decayed per-user GPU-time accounting (`gpus × runtime`), persisted across restarts
  - Reorders only within the same priority band; never overrides group concurrency limits, reservations, or resource availability
  - Live usage from running jobs is counted so long-running jobs lower their owner's share immediately
  - New `[daemon.fair_share]` config: `enabled` (default `true`) and `half_life_secs` (default 7 days), also settable via `GFLOW_DAEMON__FAIR_SHARE__*` env vars

- **Job Time Limits**: Comprehensive support for setting maximum runtime for jobs
  - New `--time` / `-t` parameter for `gbatch` command
  - Support for multiple time formats: `HH:MM:SS`, `MM:SS`, and `MM` (minutes)
  - Automatic timeout enforcement by scheduler (checked every 5 seconds)
  - New `Timeout` job state (`TO`) for jobs that exceed their time limit
  - Time limit persistence across daemon restarts
  - `TIMELIMIT` column in `gqueue` output showing job time limits or "UNLIMITED"
  - Graceful job termination via SIGINT when time limit is exceeded
  - Time limits can be specified in job scripts via `# GFLOW --time` directive
  - CLI time limits override script time limits for flexibility

- **Automatic Output Logging**: Real-time job output capture via tmux pipe-pane
  - All job output automatically logged to `~/.local/share/gflow/logs/<job_id>.log`
  - Pipe-pane enabled immediately after job session creation
  - Output captured from job start to completion/termination
  - Works for successful, failed, cancelled, and timed-out jobs
  - Automatic cleanup of pipe-pane when sessions are terminated
  - Log directory automatically created if it doesn't exist
- **Dependency Shorthand**: `gbatch --depends-on` now accepts `@` (last) and `@~N` (Nth from the end) to reference recent submissions without copying job IDs

### Changed
- **Direct-process daemon hosting hardened against PID reuse**: `gflowd`'s
  no-tmux hosting now uses an exclusive `flock` on a `gflowd.lock` file as the
  mutual-exclusion and liveness signal, instead of a bare `gflowd.pid`
  (`src/multicall/gflowd/commands/lifecycle.rs`)
  - The lock is auto-released by the kernel when the daemon crashes, so there
    is no stale-pidfile ambiguity; `status`/`down`/`restart` treat a released
    lock as "not running" and clean up stale lock/pid files
  - The lock file body records the daemon identity (`pid` + `pgid` + process
    start time), mirroring the process executor's existing guard; `down` and
    `restart` re-verify the identity right before signalling, so a PID that was
    recycled to an unrelated process is never SIGTERM/SIGKILLed
  - The directly-hosted daemon takes the lock itself (internal
    `--direct-internal` flag), so a duplicate `gflowd up` is refused even if the
    liveness probe raced
  - The old plain-PID `gflowd.pid` is no longer written or read

- **Job State Transitions**: Updated to support new `Timeout` state
  - Added `Running → Timeout` transition for time limit violations
  - Updated state transition validation logic
  - Enhanced timestamp handling for timeout state

- **Scheduler Logic**: Enhanced job monitoring and lifecycle management
  - Added timeout checking in main scheduler loop
  - Graceful job termination for timed-out jobs (Ctrl-C before state transition)
  - Improved separation of zombie job detection and timeout enforcement
  - Better error logging for timeout-related operations

- **Job Display**: Enhanced `gqueue` output options
  - New `TIMELIMIT` field showing job time limits
  - Time limits displayed in standardized `HH:MM:SS` or `D-HH:MM:SS` format
  - "UNLIMITED" displayed for jobs without time limits
  - Added `Timeout` to grouped job state displays

- Upgraded the MCP server to rmcp v3 while keeping the existing gflow tool
  names, inputs, and output schemas unchanged, and upgraded `astral-sh/setup-uv`
  to v10 and TypeScript to v7.

### Fixed
- **`gflowd up/restart/reload -c <config>` now passes the config path to the daemon**: the spawned daemon previously ignored `-c` and silently loaded the default config (`$XDG_CONFIG_HOME/gflow/gflow.toml`), so a custom port (or any other custom setting) only applied to the CLI-side health check while the daemon ran on default settings. The daemon start argv (tmux, direct, and systemd hosting) now carries `-c <path>`, keeping CLI and daemon configuration consistent.
- **`gflowd --help` now shows the `up` / `down` aliases**: `gflowd up` and `gflowd down` already worked as aliases of `start` / `stop`, but were hidden from the help output. They are now advertised inline (`start [aliases: up]`, `stop [aliases: down]`) so the help matches the commands that are actually accepted.
- **CI: stop using the retired `macos-13` runner label** in the nightly and PyPI release pipelines — the `x86_64-apple-darwin` wheel now builds on the still-supported `macos-15` (Intel) runner instead, so the nightly build no longer blocks waiting on a discontinued macOS 13 image
- Pattern matching in `gcancel` to handle new `Timeout` state
- Job struct serialization to properly persist time limit information
- Tmux session cleanup to ensure pipe-pane is disabled before session termination

### Documentation
- Added comprehensive `docs/TIME_LIMITS.md` with usage guide, examples, and FAQ
- Added `docs/QUICK_REFERENCE.md` with command cheat sheet
- Added `docs/README.md` as documentation index
- Updated main `README.md` to mention time limits and output logging features
- Included examples of time limit usage in various scenarios
- Added troubleshooting guide for timeout-related issues

## [0.4.17] - 2026-07-24

### Added
- Added native macOS wheels for Apple Silicon (`aarch64`) and Intel (`x86_64`)
  to the PyPI and nightly build matrices, with macOS included in the Rust test
  matrix.
- Added automatic Apple Silicon detection. On macOS `aarch64`, gflow exposes a
  synthetic GPU slot and accounts host and GPU memory requests against the same
  unified-memory pool.

### Changed
- Journal snapshots now serialize the scheduler's split job specification and
  runtime vectors directly, reducing snapshot allocation and serialization
  overhead. Existing snapshots using the legacy `jobs` array remain readable.
- Scheduler state counts now use the maintained state index instead of scanning
  every job.
- Upgraded the MCP server to rmcp v2 and migrated its tool router while keeping
  the existing gflow tool names, inputs, and output schemas.
- Upgraded `compact_str` to 0.10, `mockall` to 0.15, TypeScript to 7, and the
  `astral-sh/setup-uv` action to 8.3.2.

### Compatibility
- This is a backward-compatible patch release: CLI commands, HTTP endpoints,
  MCP tools, and configuration formats are unchanged.
- Existing state and journal files do not require migration. Legacy scheduler
  snapshots are converted by the existing compatibility reader.

### Known Limitations
- Apple Silicon is represented as one logical GPU slot because per-device VRAM
  is not exposed through NVML; memory scheduling therefore uses total system
  unified memory and the limits declared on each job.
- The macOS wheel and Apple Silicon paths are new in this release and must pass
  the release candidate's macOS CI jobs before the release is tagged.

## [0.3.12] - Previous Release

### Features
- Daemon-based job scheduling with persistent state
- GPU resource management via NVML
- Job dependencies with `--depends-on`
- Job arrays with `--array` parameter
- Priority-based scheduling
- Tmux integration for job execution
- RESTful HTTP API for job management
- Command-line tools: `gflowd`, `ginfo`, `gbatch`, `gqueue`, `gcancel`

### Job Management
- Job state tracking (Queued, Running, Finished, Failed, Cancelled)
- Job queue filtering and sorting
- Job dependency visualization with tree view
- Grouped job display by state
- Conda environment support

### System
- State persistence to JSON file
- Zombie job detection and cleanup
- Automatic GPU assignment and tracking
- Job logs stored per job ID

---

## Version History Notes

### Time Limit Feature Implementation Details

The time limit feature was implemented with the following components:

**Core Changes** (`src/core/job.rs`):
- Added `time_limit: Option<Duration>` field to `Job` struct
- Added `Timeout` variant to `JobState` enum
- Implemented `has_exceeded_time_limit()` method for runtime checking
- Updated `JobBuilder` to support time limit configuration

**CLI Integration** (`src/bin/gbatch/`):
- Added `--time` argument parsing in `cli.rs`
- Implemented flexible time format parser in `commands/add.rs`
- Support for script-embedded time limits
- CLI arguments override script directives

**Scheduler Enhancement** (`src/bin/gflowd/scheduler.rs`):
- Timeout checking integrated into main scheduler loop (5-second interval)
- Graceful termination via `send_ctrl_c()` before state transition
- Separate handling of timeout vs zombie job detection
- Atomic state updates with proper error handling

**Display Updates** (`src/bin/gqueue/commands/list.rs`):
- Added `TIMELIMIT` field to output format options
- Implemented `format_duration()` helper for consistent time display
- Updated grouped display to include `Timeout` state
- Dynamic column width calculation for time limit field

**Output Logging** (`src/tmux.rs`, `src/bin/gflowd/executor.rs`):
- Added `enable_pipe_pane()`, `disable_pipe_pane()`, and `is_pipe_pane_active()` methods
- Automatic pipe-pane setup during job execution
- Log file creation with proper directory handling
- Cleanup integration in session termination

### Migration Notes

- **Breaking Changes**: None. Time limits are optional and backward compatible.
- **State File**: Existing state files are compatible. Jobs without time limits show as "UNLIMITED".
- **Log Files**: Existing jobs will not have historical logs, but new jobs will automatically log output.
- **API**: Job submission API extended with optional `time_limit` field.

### Known Limitations

- Time limit enforcement accuracy: ±5 seconds (scheduler check interval)
- Single number in time format is always interpreted as minutes
- No built-in checkpoint/resume mechanism (users must implement)
- Cannot modify time limit after job submission
- Timeout state is terminal (cannot be restarted)

### Future Enhancements

Potential improvements for consideration:
- Configurable scheduler check interval for better timeout accuracy
- `REMAINING` column showing time left before timeout
- Job time limit modification for queued jobs
- Time limit warnings (e.g., 5 minutes before timeout)
- Historical time usage statistics
- Automatic checkpoint/resume on timeout
- Per-user or per-project default time limits

---

## Links

- [GitHub Repository](https://github.com/AndPuQing/gflow)
- [Issue Tracker](https://github.com/AndPuQing/gflow/issues)
- [Documentation](./docs/)
- [Crates.io](https://crates.io/crates/gflow)
