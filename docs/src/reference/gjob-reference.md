# gjob Reference

`gjob` manages existing jobs: inspect them, change queued jobs, resubmit failed work, and work with tmux sessions.

## Usage

```bash
gjob <command> [args]
gjob completion <shell>
```

## Common Examples

```bash
# Inspect a job
gjob show 42

# Tail the most recent job's log
gjob log @

# Show the first 20 log lines
gjob log @ --first 20

# Show the last 50 log lines
gjob log 42 --last 50

# Attach to a running job's tmux session
gjob attach @

# Hold or release queued jobs
gjob hold 10-12
gjob release 10,11

# Delayed release: release a held job so it starts no earlier than the given time
gjob release 12 --at now+1h
gjob release 12 --at 22:00

# Update a queued or held job
gjob update 42 --gpus 2 --time-limit 4:00:00
gjob update 42 --max-retries 2

# Redo a failed job with a larger time limit
gjob redo 42 --time 8:00:00

# Redo a failed parent and dependent jobs cancelled by that failure
gjob redo 42 --cascade

# Close tmux sessions for completed jobs
gjob close-sessions --all
```

## Commands

### `gjob attach <job>`

Attach to a job's tmux session.

Alias: `gjob a`

```bash
gjob attach <job>
```

`<job>` supports a numeric job ID or `@` for the most recent job.

### `gjob log <job>`

Print a job's log file to stdout.

Alias: `gjob l`

```bash
gjob log <job> [options]
```

`<job>` supports a numeric job ID or `@` for the most recent job.

Options:

- `-f, --first <lines>`: print only the first N lines
- `-l, --last <lines>`: print only the last N lines

### `gjob hold <job_ids>`

Put queued jobs on hold.

Alias: `gjob h`

```bash
gjob hold <job_ids>
```

`<job_ids>` supports single IDs, comma-separated lists, and ranges such as `1-3`.

### `gjob release <job_ids>`

Release held jobs back to the queue.

Alias: `gjob r`

```bash
gjob release <job_ids>
gjob release <job_ids> --at <time>
```

`<job_ids>` supports single IDs, comma-separated lists, and ranges such as `1-3`.

With `--at <time>` the job is released immediately but does not start before
the given time (delayed release): the job is queued with reason `BeginTime`
and becomes schedulable when the time arrives. `--at` accepts `HH:MM[:SS]`,
`YYYY-MM-DD[THH:MM[:SS]]`, or relative `now+N[s|m|h|d]` (minutes by default).

### `gjob show <job_ids>`

Show detailed job information including resources, dependencies, timing, and tmux session name.

Alias: `gjob s`

```bash
gjob show <job_ids>
```

`<job_ids>` supports single IDs, comma-separated lists, and ranges such as `1-3`.

When the job publishes progress (see `gjob progress`), a `Progress:` block shows
the value, percent, rate, ETA and last update. A `Log:` block reports the log
file's path, size, last-modified time and last line, so a job stuck without new
output can be told apart from a slow one. Log details require the log file to be
readable from the machine running the command.

```text
Progress:
  Value=24925/40000 (62.3%)
  Rate=0.52/s
  ETA=08:03:19 (at 09/14-22:12:04)
  Message=epoch 25/40
  Updated=09/14-14:08:45 (00:00:03 ago)
Log:
  Path=/home/alice/.local/share/gflow/logs/354.log
  Size=1.2M
  Modified=09/14-14:08:45 (last write 00:00:03 ago)
  LastLine=epoch 25/40 loss=0.412
```

### `gjob progress [<job>]`

Publish progress for a running job so `gjob show` and `gqueue` can report how
far it has come and when it is expected to finish. gflow cannot infer this on
its own, so the job has to publish it.

Alias: `gjob p`

```bash
gjob progress <job> --value <units> [--total <units>] [-m <message>]
```

Options:

- `--value <units>` (required): completed work units (steps, epochs, samples, ...)
- `--total <units>`: total work units; enables percent and ETA
- `-m, --message <text>`: short free-form status line, e.g. `epoch 25/40`
- `-s, --silent`: suppress output; use inside a job that publishes often

`<job>` may be omitted inside a gflow job: the executor exports `GFLOW_JOB_ID`,
so a training script can simply call `gjob progress --silent --value 24925 --total 40000`.

```bash
# Inside a job (job ID comes from $GFLOW_JOB_ID)
gjob progress --value 24925 --total 40000 -m "epoch 25/40"

# From outside, for a specific job
gjob progress 354 --value 24925 --total 40000
```

Publishing for a job that is not running is rejected: progress only describes
work in flight.

#### Publishing without `gjob`

A job that cannot call `gjob` can write the same document itself. The executor
exports `GFLOW_PROGRESS_FILE`, the per-job path the daemon reads:

```bash
cat > "$GFLOW_PROGRESS_FILE" <<EOF
{"value": 24925, "total": 40000, "message": "epoch 25/40"}
EOF
```

`value` is required; `total` and `message` are optional. Write an optional
`started_at_unix_secs` to measure the rate from a later point than the job's
start (for example after a long data-loading phase). The document must be
smaller than 8 KiB; the daemon ignores anything larger, unparsable, or without
a `value`.

The ETA is the work left at the observed rate, not a promise: it does not
discount time the job spent idle, so a job that paused is expected to need that
time left once it resumes.

If the job does not refresh its document for 15 minutes, the reported progress
is marked stale rather than dropped: a job that published once and then hung is
exactly what this flag is for. `gjob show` prints `[STALE]`, reports the ETA as
`<time> (~, from stale progress)` instead of a predicted finish time, and
`gqueue -f PROGRESS` appends `stale` while `-f ETA` gets a `~` suffix.

### `gjob update <job_ids>`

Update queued or held jobs in place.

Alias: `gjob u`

```bash
gjob update <job_ids> [options]
```

Options:

- `-c, --command <command>`: replace the command
- `-s, --script <path>`: replace the script path
- `-g, --gpus <count>`: change GPU count
- `-e, --conda-env <name>`: set conda environment
- `--clear-conda-env`: remove conda environment
- `-p, --priority <0-255>`: change priority
- `-t, --time-limit <time>`: change time limit
- `--clear-time-limit`: remove time limit
- `-m, --memory-limit <memory>`: change host memory limit
- `--clear-memory-limit`: remove host memory limit
- `--gpu-memory <memory>`: change per-GPU memory limit
- `--clear-gpu-memory-limit`: remove per-GPU memory limit
- `-d, --depends-on <ids>`: replace dependencies
- `--depends-on-all <ids>`: set AND dependencies
- `--depends-on-any <ids>`: set OR dependencies
- `--auto-cancel-on-dep-failure`: enable dependency failure auto-cancel
- `--no-auto-cancel-on-dep-failure`: disable dependency failure auto-cancel
- `--max-concurrent <n>`: set group max concurrency
- `--clear-max-concurrent`: remove group max concurrency
- `--max-retries <n>`: set automatic retry limit
- `--clear-max-retries`: clear automatic retry limit
- `--param <key=value>`: update templated parameters; repeatable

Notes:

- `gjob show <job>` prints `MaxRetries` when a retry limit is set.
- Automatic retries only apply to execution failures; timeouts still require manual action.

### `gjob redo <job>`

Create a new job from an existing one, optionally overriding selected fields.

```bash
gjob redo <job> [options]
```

Options:

- `-g, --gpus <count>`: override GPU count
- `-p, --priority <0-255>`: override priority
- `-d, --depends-on <job|@>`: override dependency
- `-t, --time <time>`: override time limit
- `-m, --memory <memory>`: override host memory limit
- `--gpu-memory <memory>`: override per-GPU memory limit
- `-e, --conda-env <name>`: override conda environment
- `--clear-deps`: remove dependency inherited from the original job
- `--cascade`: also redo jobs that were auto-cancelled because this job failed

`<job>` supports a numeric job ID or `@` for the most recent job.

### `gjob close-sessions`

Close tmux sessions for completed jobs by default, or use filters to target specific jobs.

Alias: `gjob close`

```bash
gjob close-sessions [options]
```

Options:

- `-j, --jobs <job_ids>`: target job IDs, ranges, or comma-separated lists
- `-s, --state <states>`: target specific states such as `finished,failed,cancelled`
- `-p, --pattern <text>`: target tmux session names containing a substring
- `-a, --all`: close all completed-job sessions except sessions for currently running jobs

Notes:

- Without filters, `gjob close-sessions` refuses to act.
- When using filters, final-state jobs are targeted by default.
- Use `--state` if you want to close sessions from non-final states.

### `gjob completion <shell>`

Generate shell completion scripts.

```bash
gjob completion bash
gjob completion zsh
gjob completion fish
```

## Formats

- Time values accept `HH:MM:SS`, `MM:SS`, or minutes as a single integer.
- Memory values accept MB integers like `512`, or units such as `1024M` and `24G`.
- GPU memory values use the same memory syntax and apply per GPU.

## See Also

- [Job Submission](../user-guide/job-submission)
- [Job Lifecycle](../user-guide/job-lifecycle)
- [Job Dependencies](../user-guide/job-dependencies)
