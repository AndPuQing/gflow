# gqueue Reference

`gqueue` lists jobs in the scheduler with filtering, formatting, and tree/group views.

## Usage

```bash
gqueue [options]
gqueue completion <shell>
```

## Common Examples

```bash
gqueue                               # active jobs (Queued, Hold, Running)
gqueue -a                            # all jobs including completed
gqueue -s Running,Queued             # filter by state
gqueue -j 12,13,14                   # filter by job IDs (comma-separated)
gqueue -u alice                      # filter by user (default: current user; use 'all' for all users)
gqueue -P ml-research                # filter by project code
gqueue -T                            # only jobs with active tmux sessions
gqueue -t                            # dependency tree view
gqueue -g                            # group by state
gqueue -w                            # auto-refresh every 2s
gqueue -w --interval 5               # auto-refresh every 5s
```

## Output Format

Default format:

```text
JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
```

Custom format:

```bash
gqueue -f JOBID,NAME,PROJECT,ST,TIMELIMIT,MEMORY,NODELIST(REASON)
```

Supported fields for `-f/--format`:

- `JOBID`
- `NAME`
- `ST`
- `TIME`
- `TIMELIMIT` (the effective time limit, or `UNLIMITED` when the job has none)
- `PROGRESS` (progress published by the job: `value/total (percent)`, `value`
  when no total, or `-`; appends `stale` when the job stopped refreshing it)
- `PERCENT` (completion percentage, or `-`)
- `ETA` (work left at the observed rate — not a promise, since idle time is not
  discounted; `done` when the total is reached, or `-`; a trailing `~` marks an
  estimate from stale progress)
- `MEMORY`
- `NODES` (GPUs requested)
- `NODELIST(REASON)` (running: GPU indices; queued/hold/cancelled: reason)
- `USER`
- `PROJECT`
- `COMMAND` (what the job runs: the stored command for command submissions, the script path for script submissions — script wins when both are present, matching the executors; `-` when neither)

Tables fit the terminal width when stdout is a terminal (widest columns are
truncated first with a `…` suffix); redirected output keeps full content.
Any field accepts a `:WIDTH` suffix in `-f` to cap its width (`COMMAND:0`
shows it in full), which disables the automatic fitting.

A job that publishes its own progress (see
[`gjob progress`](./gjob-reference)) is shown with that progress:

```bash
gqueue -f JOBID,NAME,ST,TIME,PROGRESS,PERCENT,ETA
```

```text
 JOBID   NAME         ST   TIME       PROGRESS       PERCENT   ETA
 354     infonce40k   R    08:57:37   24925/40000    62.3%     08:03:19
```

`PROGRESS`, `PERCENT` and `ETA` are `-` for jobs that publish nothing, which is
the default. A job that stops refreshing its progress is marked `stale` in
`PROGRESS` and its `ETA` gets a trailing `~`, because the estimate comes from a
measurement that is no longer current.

Example `gqueue -t` output:

```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

## Options

- `-n, --limit <N>`: show first/last N jobs (positive: first N, negative: last N, `0`: all; default: `0`)
- `-a, --all`: show all jobs including completed
- `-c, --completed`: show only completed jobs
- `--since <when>`: show jobs since `1h`, `2d`, `3w`, `today`, `yesterday`, or a timestamp
- `-r, --sort <field>`: `id`, `state`, `time`, `name`, `gpus`, `priority`
- `-s, --states <list>`: comma-separated states (e.g. `Queued,Running`)
- `-u, --user <list>`: comma-separated users (default: current user; use `all` for all users; alias: `--users`)
- `-j, --jobs <list>`: comma-separated job IDs (e.g. `1,2,3`; alias: `--job`)
- `-N, --names <list>`: comma-separated job names
- `-P, --project <code>`: filter by project code
- `-f, --format <fields>`: comma-separated output fields
- `-g, --group`: group by state
- `-t, --tree`: tree view (dependencies + redo links)
- `-T, --tmux`: only jobs with active tmux sessions
- `-w, --watch`: auto-refresh job list (default: every 2s)
- `--interval <N>`: refresh interval in seconds for `--watch` (default: `2`)
