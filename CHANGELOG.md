# Changelog

All notable changes to gflow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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

### Fixed
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
