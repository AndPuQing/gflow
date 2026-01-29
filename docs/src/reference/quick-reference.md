# gflow Quick Reference

## Daemon

```bash
gflowd up
gflowd status
gflowd down
gflowd restart
```

## Inspect + Monitor

```bash
# GPUs (availability + allocations)
ginfo

# Jobs (default: last 10)
gqueue
gqueue -a

# Useful formats
gqueue -f JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
gqueue -s Running -f JOBID,NAME,ST,NODES,NODELIST(REASON)

# Dependency tree
gqueue -t
```

Example `gqueue -t` output:
```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

## Submit Jobs (`gbatch`)

```bash
# Command
gbatch python train.py --epochs 100

# Script
gbatch train.sh

# Common options
gbatch --gpus 1 --time 2:00:00 --name train-resnet python train.py
gbatch --priority 50 python urgent.py
gbatch --conda-env myenv python script.py
gbatch --dry-run --gpus 1 python train.py
```

### Script Directives

Only these are parsed from scripts:

```bash
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --time=2:00:00
# GFLOW --memory=4G
# GFLOW --priority=20
# GFLOW --conda-env=myenv
# GFLOW --depends-on=123
```

CLI flags override script directives.

## Dependencies

```bash
# Single dependency
gbatch --depends-on <job_id|@|@~N> python next.py

# Multiple dependencies
gbatch --depends-on-all 1,2,3 python merge.py     # AND
gbatch --depends-on-any 4,5 python fallback.py    # OR

# Shorthands: @ = most recent job, @~N = Nth most recent

# Dependency failure behavior
gbatch --depends-on 123 --no-auto-cancel python next.py
```

## Arrays

```bash
gbatch --array 1-10 python process.py --task '$GFLOW_ARRAY_TASK_ID'
```

## Params (`--param`)

```bash
gbatch --param lr=0.001,0.01 --param bs=32,64 python train.py --lr {lr} --batch-size {bs}
gbatch --param-file params.csv --name-template 'run_{id}' python train.py --id {id}
```

## Control

```bash
# Cancel (use --dry-run to see dependent jobs)
gcancel <job_id>
gcancel --dry-run <job_id>

# Hold/release
gjob hold <job_id>
gjob release <job_id>

# Details / redo / update
gjob show <job_id>
gjob redo <job_id>
gjob redo <job_id> --cascade
gjob update <job_id> --gpus 2 --time-limit 4:00:00
```

## Runtime Control (`gctl`)

```bash
# Restrict which GPUs the scheduler can allocate (new allocations only)
gctl show-gpus
gctl set-gpus 0,2
gctl set-gpus all

# Group concurrency limit
gctl set-limit <job_or_group_id> 2

# Reservations (block out GPUs for a user/time window)
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
gctl reserve list --active
gctl reserve list --timeline --range 48h
gctl reserve cancel <reservation_id>
```

## Time Format (`--time`)

- `HH:MM:SS` (e.g. `2:30:00`)
- `MM:SS` (e.g. `5:30`)
- `MM` minutes (e.g. `30`)

Note: a single number is **minutes**. Use `0:30` for 30 seconds.

## States

| Code | State |
|------|-------|
| `PD` | Queued |
| `H`  | Hold |
| `R`  | Running |
| `CD` | Finished |
| `F`  | Failed |
| `CA` | Cancelled |
| `TO` | Timeout |

## Paths

```text
~/.config/gflow/gflow.toml
~/.local/share/gflow/state.json
~/.local/share/gflow/logs/<job_id>.log
```

## See Also

- [Job Submission](../user-guide/job-submission)
- [Job Dependencies](../user-guide/job-dependencies)
- [Time Limits](../user-guide/time-limits)
- [GPU Management](../user-guide/gpu-management)
