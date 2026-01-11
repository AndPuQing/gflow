# gcancel Command Reference

Complete reference for the `gcancel` command - gflow's job cancellation tool.

## Synopsis

```bash
gcancel [OPTIONS] [IDS...]
gcancel --dry-run [IDS...]
gcancel --finish <ID>
gcancel --fail <ID>
```

## Description

`gcancel` cancels one or more jobs in the gflow queue. It supports individual job IDs, ranges, and lists. The command also provides a dry-run mode to preview the impact of cancellation before executing it.

## Options

### Job Selection

#### `[IDS...]`

Job ID(s) to cancel. Supports multiple formats:

**Formats**:
- Single ID: `42`
- Multiple IDs: `1 2 3` or `1,2,3`
- Ranges: `1-5` (IDs 1, 2, 3, 4, 5)
- Mixed: `1,3,5-7,10`

**Examples**:
```bash
# Cancel single job
gcancel 42

# Cancel multiple jobs
gcancel 1 2 3
gcancel 1,2,3

# Cancel range
gcancel 1-5

# Cancel mixed
gcancel 1,3,5-7,10
```

### Preview Mode

#### `--dry-run`

Preview cancellation without executing.

**Features**:
- Shows which jobs would be cancelled
- Identifies dependent jobs that will be affected
- Validates job IDs before cancellation
- Safe to run - makes no changes

**Example**:
```bash
$ gcancel --dry-run 1
Would cancel job 1 (data-prep)

⚠️  Warning: The following jobs depend on job 1:
  - Job 2 (train-model)
  - Job 3 (evaluate)

These jobs will never start if job 1 is cancelled.

To proceed with cancellation, run:
  gcancel 1
```

**Use cases**:
- Check impact before cancelling
- Verify job IDs are correct
- Understand dependency chains
- Plan cleanup operations

### Internal State Management (Hidden)

These options are used internally by gflow and are not intended for direct user interaction.

#### `--finish <ID>`

Mark job as finished (internal use only).

**Example**:
```bash
gcancel --finish 42
```

**Note**: Used by the system to transition job states. Not recommended for manual use.

#### `--fail <ID>`

Mark job as failed (internal use only).

**Example**:
```bash
gcancel --fail 42
```

**Note**: Used by the system to transition job states. Not recommended for manual use.

### Global Options

#### `--config <PATH>`

Use custom configuration file (hidden option).

**Example**:
```bash
gcancel --config /path/to/custom.toml 42
```

#### `--help`, `-h`

Display help message.

```bash
$ gcancel --help
<!-- cmdrun gcancel --help -->
```

#### `--version`, `-V`

Display version information.

```bash
$ gcancel --version
<!-- cmdrun gcancel --version -->
```

## Behavior

### Successful Cancellation

When a job is successfully cancelled:

1. Job state changes to `Cancelled` (CA)
2. If the job is running:
   - tmux session receives `Ctrl-C` (SIGINT)
   - Job process is gracefully interrupted
   - Session is cleaned up
3. If the job is queued:
   - Job is removed from the run queue
   - State changes immediately
4. Output is captured to log file
5. Finish time is recorded

**Example**:
```bash
$ gcancel 42
Job 42 cancelled successfully

$ gqueue -j 42
JOBID    NAME      ST    TIME
42       my-job    CA    00:05:23
```

### Dependent Jobs

Cancelling a job affects dependent jobs:

- Dependent jobs remain in `Queued` state
- They will **never** start automatically
- You must manually cancel them

**Example**:
```bash
# Job 2 depends on Job 1
$ gqueue -t
JOBID    NAME      ST
1        prep      R
└─ 2     train     PD

# Cancel job 1
$ gcancel 1
Job 1 cancelled

# Job 2 is now orphaned
$ gqueue -t
JOBID    NAME      ST
1        prep      CA
└─ 2     train     PD    # Will never start

# Must cancel job 2 manually
$ gcancel 2
```

### Already Completed Jobs

Cannot cancel finished jobs:

```bash
$ gcancel 42
Error: Job 42 is already in terminal state (Finished)
```

**Terminal states** (cannot be cancelled):
- `Finished` (CD)
- `Failed` (F)
- `Cancelled` (CA)
- `Timeout` (TO)

### Non-existent Jobs

```bash
$ gcancel 999
Error: Job 999 not found
```

## Examples

### Basic Cancellation

```bash
# Cancel single job
gcancel 42

# Cancel multiple jobs
gcancel 1 2 3
gcancel 1,2,3

# Cancel range
gcancel 10-20

# Cancel mixed
gcancel 1,5,10-15,20
```

### Preview Before Cancelling

```bash
# Check what would happen
gcancel --dry-run 5

# Read output carefully
# If acceptable, proceed
gcancel 5
```

### Cancel Pipeline

```bash
# View pipeline
$ gqueue -t
JOBID    NAME      ST
1        prep      R
├─ 2     train-a   PD
└─ 3     train-b   PD

# Cancel entire pipeline
gcancel 1,2,3

# Or cancel parent and children separately
gcancel 1
gcancel 2 3
```

### Cancel All Running Jobs

```bash
# Get running job IDs
RUNNING=$(gqueue -s Running -f JOBID | tail -n +2 | tr '\n' ',' | sed 's/,$//')

# Cancel them
gcancel $RUNNING
```

### Cancel Queued Jobs

```bash
# Get queued job IDs
QUEUED=$(gqueue -s Queued -f JOBID | tail -n +2)

# Cancel each one
for job in $QUEUED; do
    gcancel $job
done
```

### Conditional Cancellation

```bash
# Cancel if job is taking too long
JOB_ID=42
ELAPSED=$(gqueue -j $JOB_ID -f TIME | tail -n 1 | cut -d: -f1)

if [ "$ELAPSED" -gt 2 ]; then
    echo "Job taking too long, cancelling..."
    gcancel $JOB_ID
fi
```

## Common Patterns

### Cancel and Resubmit

```bash
# Cancel old job
gcancel 42

# Resubmit with corrections
gbatch --gpus 1 --time 2:00:00 python train.py --fixed
```

### Cancel Failed Dependencies

```bash
# Find failed job
$ gqueue -s Failed
JOBID    NAME      ST
5        prep      F

# Cancel dependent jobs (they won't start anyway)
$ gqueue -t | grep -A10 "^5"
5        prep      F
└─ 6     train     PD

$ gcancel 6
```

### Emergency Stop

```bash
# Stop all running and queued jobs
gcancel $(gqueue -s Running,Queued -f JOBID | tail -n +2)
```

### Selective Cancellation

```bash
# Cancel low-priority queued jobs
gqueue -s Queued -r priority -f JOBID,PRIORITY | awk '$2 < 10 {print $1}' | xargs gcancel
```

## Integration Examples

### Script: Safe Cancellation

```bash
#!/bin/bash
# safe_cancel.sh - Cancel with dependency check

JOB_ID=$1

# Check dependencies
gcancel --dry-run $JOB_ID

read -p "Proceed with cancellation? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    gcancel $JOB_ID
    echo "Job $JOB_ID cancelled"
else
    echo "Cancellation aborted"
fi
```

### Script: Cancel Pipeline

```bash
#!/bin/bash
# cancel_pipeline.sh - Cancel all jobs in a pipeline

ROOT_JOB=$1

# Get all dependent jobs
JOBS=$(gqueue -t | awk -v root=$ROOT_JOB '
    $1 == root || seen {
        seen = 1
        print $1
    }
')

echo "Jobs to cancel: $JOBS"
gcancel $JOBS
```

### Script: Timeout Watcher

```bash
#!/bin/bash
# timeout_watcher.sh - Cancel jobs exceeding expected time

MAX_TIME=120  # 2 hours in minutes

gqueue -s Running -f JOBID,TIME | tail -n +2 | while read -r jobid time; do
    # Convert time to minutes
    IFS=: read -r h m s <<< "$time"
    minutes=$((10#$h * 60 + 10#$m))

    if [ $minutes -gt $MAX_TIME ]; then
        echo "Job $jobid exceeded $MAX_TIME minutes, cancelling..."
        gcancel $jobid
    fi
done
```

## Troubleshooting

### Issue: Cannot cancel job

**Possible causes**:
1. Job already in terminal state
2. Job ID doesn't exist
3. Permission issues

**Solutions**:
```bash
# Check job state
gqueue -j <job_id> -f JOBID,ST

# Verify job exists
gqueue -a | grep <job_id>

# Check daemon status
ginfo
```

### Issue: Dependent jobs not cancelled

**Expected behavior**: gcancel only cancels specified jobs, not dependents.

**Solution**: Cancel dependents manually:
```bash
# Use dry-run to see dependents
gcancel --dry-run 1

# Cancel parent and children
gcancel 1,2,3
```

### Issue: Job still running after cancellation

**Possible causes**:
1. Job is handling SIGINT gracefully (saving state)
2. tmux session cleanup delay
3. Job process not responding

**Solutions**:
```bash
# Wait a few seconds
sleep 5
gqueue -j <job_id>

# Check tmux session
tmux ls

# Force kill tmux session if needed
tmux kill-session -t <session_name>
```

### Issue: Range parsing error

**Example**:
```bash
gcancel 1-5,10
```

**Solution**: Check range syntax:
- Ranges: `1-5` (valid)
- Lists: `1,2,3` (valid)
- Mixed: `1-5,10` (depends on implementation)

## Best Practices

1. **Use dry-run first** for important cancellations
   ```bash
   gcancel --dry-run 42
   gcancel 42
   ```

2. **Check dependencies** before cancelling parent jobs
   ```bash
   gqueue -t
   ```

3. **Cancel gracefully** during development/testing
   - Jobs can save checkpoints
   - Logs are preserved

4. **Clean up dependents** when cancelling parent jobs
   ```bash
   gcancel 1,2,3  # parent and children
   ```

5. **Monitor cancellation** to ensure it completes
   ```bash
   gcancel 42
   watch -n 1 'gqueue -j 42'
   ```

6. **Log cancellations** for audit trail
   ```bash
   echo "$(date): Cancelled job 42" >> ~/gflow-cancellations.log
   gcancel 42
   ```

7. **Use job names** to identify which jobs to cancel
   ```bash
   gqueue -N "old-experiment*"
   gcancel <identified_ids>
   ```

8. **Avoid cancelling system jobs** (if any)
   - Be careful with automated cancellation scripts

## Error Messages

### Common Errors

```bash
# Job not found
Error: Job 999 not found
```

```bash
# Job already terminal
Error: Job 42 is already in terminal state (Finished)
```

```bash
# Invalid job ID
Error: Invalid job ID: abc
```

```bash
# No job ID provided
Error: No job IDs specified
Usage: gcancel [OPTIONS] [IDS...]
```

```bash
# Daemon not running
Error: Could not connect to gflowd (connection refused)
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (job not found, already terminal, etc.) |
| 2 | Invalid arguments |

## See Also

- [gbatch](./gbatch-reference) - Job submission reference
- [gqueue](./gqueue-reference) - Job queue reference
- [ginfo](./ginfo-reference) - Scheduler inspection reference
- [Quick Reference](./quick-reference) - Command cheat sheet
- [Job Submission](../user-guide/job-submission) - Job submission guide
- [Job Dependencies](../user-guide/job-dependencies) - Dependency management
