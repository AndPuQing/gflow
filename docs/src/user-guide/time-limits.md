# Time Limits

Set a time limit to prevent jobs from running forever. When a running job exceeds its limit, gflow stops it and marks it as `Timeout` (`TO`).

## Set a Time Limit

With `gbatch`:

```bash
gbatch --time <TIME> python train.py
```

In a script (directive):

```bash
#!/bin/bash
# GFLOW --time 2:00:00

python train.py
```

Command-line flags override script directives.

## Time Formats

`<TIME>` accepts:

- `HH:MM:SS` (e.g. `2:30:00`)
- `MM:SS` (e.g. `5:30`)
- `MM` minutes (e.g. `30`)

Note: a single number is **minutes** (so `--time 30` means 30 minutes, not 30 seconds). Use `0:30` for 30 seconds.

## Inspect Time Limits

```bash
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
gjob show <job_id>
```

## Behavior

- The timer starts when a job enters `Running` (queue time is not counted).
- Enforcement is periodic (jobs may run slightly past the exact limit).
- On timeout, gflow sends an interrupt (Ctrl-C / SIGINT) and transitions the job to `Timeout` (`TO`).

## Troubleshooting

### Job timed out

Increase the limit and resubmit:

```bash
gjob redo <job_id> --time 4:00:00
```

### Job ends earlier than expected

Double-check the format (minutes vs seconds):

```bash
gbatch --time 0:30 sleep 1000   # 30 seconds
gbatch --time 30 sleep 1000     # 30 minutes
```

### Timeouts not happening

```bash
ginfo
gqueue -j <job_id> -f JOBID,ST,TIMELIMIT
```

## See Also

- [Job Lifecycle](./job-lifecycle) - Job states (including `TO`)
- [Job Submission](./job-submission) - Submission options
- [Quick Reference](../reference/quick-reference) - Command cheat sheet
