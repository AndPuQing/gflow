# Job Dependencies

Job dependencies let you build workflows where a job waits for other job(s) to finish before it can start.

## Quick Start

```bash
gbatch --time 10 python preprocess.py
gbatch --depends-on @ --gpus 1 --time 4:00:00 python train.py
gbatch --depends-on @ --time 10 python evaluate.py
```

## Dependency Options

### Single dependency

```bash
gbatch --depends-on <job_id|@|@~N> python next.py
```

Shorthands:
- `@`: the most recently submitted job
- `@~N`: the Nth most recent submission (e.g. `@~1` is the previous job)

### Multiple dependencies (AND / OR)

```bash
# AND: all parents must finish successfully
gbatch --depends-on-all 101,102,103 python merge.py

# OR: any one parent finishing successfully is enough
gbatch --depends-on-any 201,202,203 python process_first_success.py
```

`@` shorthands also work in lists (e.g. `--depends-on-all @,@~1,@~2`).

### In scripts (directive)

Script directives support only `--depends-on` (single dependency):

```bash
#!/bin/bash
# GFLOW --depends-on=123

python next.py
```

## Auto-cancellation

By default, if a dependency fails/cancels/times out, dependent jobs are auto-cancelled. Disable this if you want them to stay queued:

```bash
gbatch --depends-on <job_id> --no-auto-cancel python next.py
```

When auto-cancel is disabled, the dependent job will not start automatically even after the parent fails; you must cancel or resubmit it.

## Monitor Dependencies

```bash
# Tree view
gqueue -t
```

Example output:
```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

```bash
# Focus on a subset
gqueue -j <job_id>,<job_id> -t

# Why a queued job is waiting
gqueue -s Queued -f JOBID,NAME,ST,NODELIST(REASON)
```

## Troubleshooting

### Dependent job not starting

```bash
gqueue -t
gqueue -j <parent_job_id> -f JOBID,ST
ginfo
```

### Redo a whole chain after fixing a failure

```bash
gjob redo <job_id> --cascade
```

## See Also

- [Job Lifecycle](./job-lifecycle) - Reasons and state transitions
- [Time Limits](./time-limits) - Prevent runaway jobs
