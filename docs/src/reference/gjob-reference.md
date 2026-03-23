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

# Update a queued or held job
gjob update 42 --gpus 2 --time-limit 4:00:00

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
```

`<job_ids>` supports single IDs, comma-separated lists, and ranges such as `1-3`.

### `gjob show <job_ids>`

Show detailed job information including resources, dependencies, timing, and tmux session name.

Alias: `gjob s`

```bash
gjob show <job_ids>
```

`<job_ids>` supports single IDs, comma-separated lists, and ranges such as `1-3`.

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
- `--param <key=value>`: update templated parameters; repeatable

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
