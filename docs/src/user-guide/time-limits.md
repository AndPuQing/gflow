# Job Time Limits in gflow

## Overview

gflow supports setting time limits for jobs, similar to Slurm's `sbatch --time` parameter. When a job exceeds its specified time limit, the scheduler automatically terminates it and marks it with a `Timeout` status. This feature helps prevent runaway jobs from consuming resources indefinitely and facilitates better resource planning.

## Table of Contents

- [Basic Usage](#basic-usage)
- [Time Format Specifications](#time-format-specifications)
- [Behavior and Enforcement](#behavior-and-enforcement)
- [Job States](#job-states)
- [Examples](#examples)
- [Best Practices](#best-practices)
- [FAQ](#faq)

## Basic Usage

### Setting Time Limits with `gbatch`

Use the `--time` (or `-t`) flag when submitting jobs:

```bash
gbatch --time <TIME_SPEC> your_command
```

### In Job Scripts

You can also specify time limits directly in your job scripts using the `# GFLOW` directive:

```bash
#!/bin/bash
# GFLOW --time 2:00:00
# GFLOW --gpus 1

echo "Starting training..."
python train.py
```

Note: Command-line arguments take precedence over script directives.

## Time Format Specifications

gflow supports multiple time format specifications for flexibility:

| Format | Description | Example | Equivalent Duration |
|--------|-------------|---------|---------------------|
| `HH:MM:SS` | Hours:Minutes:Seconds | `2:30:45` | 2 hours, 30 minutes, 45 seconds |
| `MM:SS` | Minutes:Seconds | `45:30` | 45 minutes, 30 seconds |
| `MM` | Minutes only | `30` | 30 minutes |

### Format Details

#### `HH:MM:SS` Format
- **Use case**: Long-running jobs (hours)
- **Example**: `--time 1:30:00` (1.5 hours)
- **Range**: Any valid combination of hours, minutes, and seconds

#### `MM:SS` Format
- **Use case**: Medium-duration jobs (minutes)
- **Example**: `--time 45:30` (45 minutes, 30 seconds)
- **Note**: Two numbers separated by colon are interpreted as minutes:seconds

#### `MM` Format
- **Use case**: Short jobs (minutes)
- **Example**: `--time 10` (10 minutes)
- **Note**: A single number is always interpreted as minutes, not seconds

### Examples by Format

```bash
# 2 hours
gbatch --time 2:00:00 long_simulation

# 30 minutes
gbatch --time 30 medium_task

# 5 minutes 30 seconds
gbatch --time 5:30 quick_task

# 12 hours 45 minutes
gbatch --time 12:45:00 overnight_job
```

## Scheduling Benefits

### Time Limits Improve Scheduling Priority

When multiple jobs are queued with the same priority level, gflow uses a multi-factor scheduling algorithm:

1. **User Priority** (Primary factor)
2. **Time Limit Bonus** (Secondary factor)
3. **Submission Order** (Tie-breaker)

**How the Time Bonus Works**:

- **Unlimited jobs**: Receive lowest scheduling bonus
- **Time-limited jobs**: Receive higher scheduling bonus
- **Shorter jobs**: Receive even higher bonus within time-limited jobs

This means setting a time limit provides two benefits:
- ✅ **Safety**: Prevents runaway jobs from consuming resources indefinitely
- ✅ **Priority**: Your job runs sooner when competing with unlimited jobs

**Example Scheduling Order**:

```bash
# Assume all jobs have priority=10 and are submitted in this order:

gbatch --priority 10 --time 10 quick.py        # Runs 1st (shortest)
gbatch --priority 10 --time 1:00:00 medium.py  # Runs 2nd (medium)
gbatch --priority 10 --time 8:00:00 long.py    # Runs 3rd (long limit)
gbatch --priority 10 unlimited.py              # Runs 4th (no limit)
```

**Key Insight**: Even a very generous time limit (e.g., 24 hours) gives your job an advantage over unlimited jobs at the same priority level. Setting realistic time limits is a win-win!

### Scheduling Priority Details

The time bonus uses the following formula:
- No time limit: Bonus = 100
- With time limit: Bonus = 200-300 (based on duration)
  - Very short jobs (seconds-minutes): ~300
  - Medium jobs (hours): ~250
  - Long jobs (≥24 hours): ~200

User priority is multiplied by 1000, so it always dominates the scheduling decision. Time bonuses only matter when jobs have equal priority.

## Behavior and Enforcement

### How Time Limits Work

1. **Job Submission**: When you submit a job with `--time`, the time limit is stored with the job metadata.

2. **Execution Start**: When the job starts running, the scheduler records the start time.

3. **Monitoring**: The scheduler checks all running jobs every 5 seconds for timeout violations.

4. **Timeout Detection**: If a job's elapsed time exceeds its time limit, the scheduler:
   - Logs a warning: `Job <id> has exceeded time limit, terminating...`
   - Sends `Ctrl-C` to the job's tmux session (graceful interrupt)
   - Transitions the job to `Timeout` state
   - Records the finish time

5. **Post-Termination**: The job's output is preserved in the log file, and the session is cleaned up.

### Graceful vs Forceful Termination

- **Graceful Termination**: gflow sends `Ctrl-C` (SIGINT) first, giving jobs a chance to:
  - Save checkpoints
  - Close file handles
  - Clean up temporary files
  - Log final states

- **Forceful Cleanup**: If the tmux session doesn't respond, it will be killed when the job is fully cancelled or the daemon is stopped.

### Accuracy and Timing

- **Check Interval**: 5 seconds (jobs may run up to 5 seconds past their limit)
- **Tolerance**: Jobs are terminated as soon as the next check detects the timeout
- **Precision**: Sub-second timing is recorded, but enforcement happens at 5-second intervals

## Job States

### Timeout State (`TO`)

When a job exceeds its time limit, it transitions to the `Timeout` state:

```bash
$ gqueue -j 42
JOBID    NAME             ST    TIME         TIMELIMIT
42       my-long-job      TO    00:10:05     00:10:00
```

Key characteristics:
- **State Code**: `TO` (Timeout)
- **Terminal State**: Job will not restart or continue
- **Distinguishable**: Different from `F` (Failed) and `CA` (Cancelled)
- **Logged**: Timeout event is recorded in daemon logs

### State Transitions

```
Queued ──→ Running ──→ Finished
            │
            ├──→ Failed
            ├──→ Cancelled
            └──→ Timeout (new)
```

Valid transitions to `Timeout`:
- ✅ `Running` → `Timeout` (time limit exceeded)
- ❌ `Queued` → `Timeout` (not possible)
- ❌ `Finished` → `Timeout` (terminal state)

## Examples

### Example 1: Training Job with Time Limit

```bash
# Submit a training job with 2-hour limit
gbatch --time 2:00:00 \
       --gpus 1 \
       python train.py --epochs 100
```

**Output**:
```
Submitted batch job 42 (elegant-mountain-1234)
```

**Check status**:
```bash
$ gqueue -j 42 -f JOBID,NAME,ST,TIME,TIMELIMIT
JOBID    NAME                   ST    TIME         TIMELIMIT
42       elegant-mountain-1234  R     00:15:23     02:00:00
```

### Example 2: Job that Times Out

```bash
# Submit a job that will exceed its limit
gbatch --time 0:10 \
       sleep 1000  # Will run for 1000 seconds
```

**After 10 seconds**:
```bash
$ gqueue -j 43 -f JOBID,NAME,ST,TIME,TIMELIMIT
JOBID    NAME                ST    TIME         TIMELIMIT
43       quiet-river-5678    TO    00:00:13     00:00:10
```

**Log output** (`~/.local/share/gflow/logs/43.log`):
```
Line 1
Line 2
...
^C
```

### Example 3: Job Array with Time Limits

```bash
# Submit array of jobs, each with 30-minute limit
gbatch --time 30 \
       --array 1-10 \
       python process.py --task \$GFLOW_ARRAY_TASK_ID
```

Each job in the array inherits the same 30-minute time limit.

### Example 4: Dependency Chain with Time Limits

```bash
# Job 1: Data preprocessing (1 hour)
gbatch --time 1:00:00 \
       --name "preprocess" \
       python preprocess.py


# Job 2: Training (4 hours), depends on Job 1
gbatch --time 4:00:00 \
       --depends-on 1 \
       --name "training" \
       python train.py

# Job 3: Evaluation (30 minutes), depends on Job 2
gbatch --time 30 \
       --depends-on 2 \
       --name "evaluation" \
       python evaluate.py \
```

### Example 5: Job Script with Time Limit

Create `experiment.sh`:
```bash
#!/bin/bash
# GFLOW --time 3:00:00
# GFLOW --gpus 2
# GFLOW --priority 20

echo "Starting experiment at $(date)"
python run_experiment.py --config config.yaml
echo "Experiment finished at $(date)"
```

Submit:
```bash
gbatch experiment.sh
```

Override time limit from command line:
```bash
# This overrides the script's 3-hour limit
gbatch --time 1:00:00 experiment.sh
```

## Best Practices

### 1. Set Realistic Time Limits

- **Estimate Runtime**: Add 10-20% buffer to your expected runtime
- **Account for Variability**: Consider dataset size, hardware performance
- **Too Short**: Jobs terminate prematurely, wasting computation
- **Too Long**: Doesn't help catch runaway jobs

```bash
# Bad: Too tight
gbatch --time 10 python train.py  # Training takes ~12 minutes

# Good: Reasonable buffer
gbatch --time 15 python train.py  # Allows 25% buffer
```

### 2. Use Time Limits for All Production Jobs

```bash
# Bad: No limit, could run forever
gbatch python train.py

# Good: Always specify limits
gbatch --time 4:00:00 python train.py
```

### 3. Implement Checkpointing

Time limits work best with checkpointing:

```python
# Your training script
import signal
import sys

def signal_handler(sig, frame):
    print('Received interrupt, saving checkpoint...')
    model.save_checkpoint('checkpoint.pth')
    sys.exit(0)

signal.signal(signal.SIGINT, signal_handler)

# Training loop
for epoch in range(epochs):
    train_epoch()
    if epoch % 10 == 0:
        model.save_checkpoint(f'checkpoint_epoch_{epoch}.pth')
```

### 4. Monitor Time Usage

Use `gqueue` to monitor job progress:

```bash
# Watch jobs with time limits
watch -n 5 'gqueue -s Running -f JOBID,NAME,ST,TIME,TIMELIMIT'
```

### 5. Adjust Limits Based on History

After running jobs, analyze their runtime:

```bash
# Check completed job runtime
gqueue -j 42 -f JOBID,TIME,TIMELIMIT

# Adjust future jobs based on actual runtime
```

### 6. Different Limits for Different Stages

```bash
# Quick preprocessing
gbatch --time 10 --name "preprocess" python preprocess.py

# Long training
gbatch --time 8:00:00 --depends-on <prep_id> --name "training" python train.py

# Quick evaluation
gbatch --time 5 --name "evaluation" --depends-on <train_id> python evaluate.py
```

## Displaying Time Limits

### Using `gqueue`

Show time limit column:
```bash
# Include TIMELIMIT in output
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
```

**Output**:
```
JOBID    NAME                ST    TIME         TIMELIMIT
42       training-job        R     01:23:45     04:00:00
43       quick-task          CD    00:02:15     00:10:00
44       unlimited-job       R     00:45:12     UNLIMITED
```

### Time Display Formats

- **With Days**: `D-HH:MM:SS` (e.g., `2-04:30:00` = 2 days, 4.5 hours)
- **Without Days**: `HH:MM:SS` (e.g., `04:30:00` = 4.5 hours)
- **Unlimited**: `UNLIMITED` (no time limit set)

### Filtering by State

View all timed-out jobs:
```bash
gqueue -s Timeout -f JOBID,NAME,TIME,TIMELIMIT
```

## FAQ

### Q: What happens if I don't specify a time limit?

**A**: The job runs without any time restrictions (`UNLIMITED`). It will run until:
- It completes successfully
- It fails due to an error
- You manually cancel it with `gcancel`
- The system crashes or daemon stops

### Q: Can I change a job's time limit after submission?

**A**: Currently, no. Time limits are set at submission time and cannot be modified for queued or running jobs. You would need to:
1. Cancel the current job
2. Resubmit with a new time limit

### Q: What's the maximum time limit?

**A**: There's no hard maximum, but practical limits depend on:
- System stability (days/weeks)
- Resource availability
- Your specific use case

Example of a very long limit:
```bash
gbatch --time 168:00:00 week_long_simulation # 1 week
```

### Q: Will my job save its work when it times out?

**A**: It depends on your job's implementation:
- **With SIGINT handler**: Yes, if you catch `Ctrl-C` and save state
- **Without handler**: Possibly not, job is interrupted immediately
- **Best practice**: Implement periodic checkpointing

### Q: How do I see why a job timed out?

**A**: Check multiple sources:

1. **Job status**:
   ```bash
   gqueue -j <job_id>
   ```

2. **Job logs**:
   ```bash
   cat ~/.local/share/gflow/logs/<job_id>.log
   ```

3. **Daemon logs** (if attached to tmux):
   ```bash
   tmux attach -t gflow_server
   ```

### Q: Can I set different time limits for job arrays?

**A**: Currently, all jobs in an array share the same time limit. To have different limits:
- Submit jobs individually, or
- Implement conditional logic in your script based on `$GFLOW_ARRAY_TASK_ID`

### Q: What's the difference between Timeout (TO) and Failed (F)?

| Aspect | Timeout (TO) | Failed (F) |
|--------|-------------|-----------|
| **Cause** | Exceeded time limit | Job crashed/error |
| **Initiated by** | Scheduler | Job itself |
| **Exit code** | SIGINT (130) | Variable (job-dependent) |
| **Planned** | Yes (hit limit) | No (unexpected) |

### Q: Does the time limit include queue time?

**A**: No, only running time counts. The timer starts when the job transitions from `Queued` to `Running` state.

```
Queued (not counted) → Running (timer starts) → Timeout/Finished
```

### Q: How accurate is the timeout enforcement?

**A**: Within 5 seconds. The scheduler checks every 5 seconds, so:
- **Limit**: 10:00
- **Actual termination**: Between 10:00 and 10:05

For most use cases, this accuracy is sufficient.

### Q: What if the daemon restarts while jobs are running?

**A**: Time limits are preserved:
1. Job state (including start time) is saved to disk
2. When daemon restarts, it reloads job state
3. Time limit checking resumes automatically
4. Jobs that exceeded limits during downtime will be caught on next check

### Q: Can I see time remaining for a running job?

**A**: Not directly, but you can calculate it:

```bash
# Show TIME and TIMELIMIT columns
gqueue -j <job_id> -f JOBID,NAME,TIME,TIMELIMIT

# Calculate: TIMELIMIT - TIME = Remaining time
```

A future enhancement could add a `REMAINING` column.

## Troubleshooting

### Job Terminates Earlier Than Expected

**Possible causes**:
1. **Wrong time format**:
   - ❌ `--time 30` thinking it's seconds (it's actually 30 minutes)
   - ✅ `--time 0:30` for 30 seconds

2. **Time limit too strict**: Check actual runtime of previous jobs

3. **Job failed for other reasons**: Check logs and job state

### Job Doesn't Terminate at Time Limit

**Possible causes**:
1. **No time limit set**: Verify with `gqueue -f TIMELIMIT`
2. **Daemon not running**: Check `ginfo`
3. **Job not in Running state**: Time limits only apply to running jobs

### Time Limit Not Showing in `gqueue`

```bash
# Make sure to include TIMELIMIT in format
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT

# Or check default columns for your gflow version
```

## Implementation Details

For developers and advanced users:

### Architecture

- **Storage**: `time_limit` field in `Job` struct as `Option<Duration>`
- **Checking**: Scheduler loop every 5 seconds
- **Method**: `Job::has_exceeded_time_limit()` compares elapsed vs limit
- **Termination**: `send_ctrl_c()` → transition to `Timeout` state

### State Persistence

Time limits are persisted in `~/.local/share/gflow/state.json`:

```json
{
  "id": 42,
  "time_limit": {
    "secs": 3600,
    "nanos": 0
  },
  "started_at": {
    "secs_since_epoch": 1234567890,
    "nanos_since_epoch": 0
  }
}
```

### Scheduler Code

Located in `src/bin/gflowd/scheduler.rs:242-267`, the timeout checking logic runs every scheduler cycle.

## Related Features

- **Job Dependencies**: Combine with `--depends-on` for complex workflows
- **Job Priorities**: Use with `--priority` for important time-sensitive jobs
- **Job Arrays**: Apply time limits to parallel task batches
- **Output Logging**: All output captured via `pipe-pane` to log files

## See Also

- [Introduction](/) - Main documentation
- [Job Dependencies](./job-dependencies) - Managing job dependencies
- [GPU Management](./gpu-management) - GPU resource management
- GitHub Issues - Report problems or request features
