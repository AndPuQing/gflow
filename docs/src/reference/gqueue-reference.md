# gqueue Reference

`gqueue` lists jobs in the scheduler with filtering, formatting, and tree/group views.

## Usage

```bash
gqueue [options]
gqueue completion <shell>
```

## Common Examples

```bash
gqueue                               # last 10 jobs
gqueue -a                            # all jobs
gqueue -s Running,Queued             # filter by state
gqueue -j 12,13,14                   # filter by job IDs (comma-separated)
gqueue -T                            # only jobs with active tmux sessions
gqueue -t                            # dependency tree view
gqueue -g                            # group by state
```

## Output Format

Default format:

```text
JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
```

Custom format:

```bash
gqueue -f JOBID,NAME,ST,TIMELIMIT,MEMORY,NODELIST(REASON)
```

Supported fields for `-f/--format`:

- `JOBID`
- `NAME`
- `ST`
- `TIME`
- `TIMELIMIT`
- `MEMORY`
- `NODES` (GPUs requested)
- `NODELIST(REASON)` (running: GPU indices; queued/hold/cancelled: reason)
- `USER`

Example `gqueue -t` output:

```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

## Options

- `-n, --limit <N>`: show first/last N jobs (default: `-10`; `0` = all)
- `-a, --all`: show all jobs (`-n 0`)
- `-c, --completed`: show only completed jobs
- `--since <when>`: show jobs since `1h`, `2d`, `3w`, `today`, `yesterday`, or a timestamp
- `-r, --sort <field>`: `id`, `state`, `time`, `name`, `gpus`, `priority`
- `-s, --states <list>`: comma-separated states (e.g. `Queued,Running`)
- `-j, --jobs <list>`: comma-separated job IDs (e.g. `1,2,3`)
- `-N, --names <list>`: comma-separated job names
- `-f, --format <fields>`: comma-separated output fields
- `-g, --group`: group by state
- `-t, --tree`: tree view (dependencies + redo links)
- `-T, --tmux`: only jobs with active tmux sessions

