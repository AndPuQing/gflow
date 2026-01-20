# gflow Quick Reference Card

## Essential Commands

### Daemon Management
```bash
gflowd up                  # Start the scheduler daemon
gflowd down                # Stop the scheduler daemon
ginfo              # Check daemon status and GPU allocation
watch -n 2 ginfo   # Monitor scheduler state
```

### Job Submission
```bash
# Basic submission
gbatch python script.py
gbatch my_script.sh

# With GPU
gbatch --gpus 1 python train.py

# With time limit
gbatch --time 2:00:00 python train.py    # 2 hours
gbatch --time 30 python train.py         # 30 minutes
gbatch --time 5:30 python train.py       # 5 min 30 sec

# With dependencies
gbatch --depends-on 123 python process.py

# With priority
gbatch --priority 100 python urgent.py

# Job arrays
gbatch --array 1-10 python task.py

# Conda environment
gbatch --conda-env myenv python script.py

# Combined options
gbatch --gpus 2 --time 4:00:00 --priority 50 \
       python train.py
```

### Job Script Format
```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 2:00:00
# GFLOW --priority 20
# GFLOW --conda-env myenv

echo "Starting job..."
python train.py
```

### Querying Jobs
```bash
# Basic listing
gqueue                           # Show last 10 jobs
gqueue -a                        # Show all jobs
gqueue -n 20                     # Show last 20 jobs

# Filter by state
gqueue -s Running                # Running jobs only
gqueue -s Queued,Running         # Multiple states

# Filter by job ID
gqueue -j 42                     # Specific job
gqueue -j 40,41,42               # Multiple jobs (comma-separated)
gqueue -j 40-45                  # Job ID range (40, 41, 42, 43, 44, 45)

# Custom format
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
gqueue -f JOBID,NAME,ST,NODES,NODELIST

# Sort options
gqueue -r id                     # Sort by ID (default)
gqueue -r time                   # Sort by start time
gqueue -r priority               # Sort by priority

# Group by state
gqueue -g                        # Group jobs by state

# Dependency tree
gqueue -t                        # Show job dependency tree
```

### Job Control
```bash
# Cancel jobs
gcancel <job_id>                 # Cancel a job
gcancel 40,41,42                 # Cancel multiple jobs (comma-separated)
gcancel 40-45                    # Cancel job ID range (40 through 45)
gcancel --dry-run <job_id>       # Preview cancellation impact (shows dependents)
gcancel --finish <job_id>        # Mark job as finished (internal)
gcancel --fail <job_id>          # Mark job as failed (internal)

# Hold and release jobs
gjob hold <job_id>               # Put queued job on hold
gjob release <job_id>            # Release held job back to queue

# Redo jobs
gjob redo <job_id>               # Resubmit job with same parameters
gjob redo <job_id> --gpus 4      # Resubmit with modified GPU count
gjob redo <job_id> --time 8:00:00 # Resubmit with longer time limit
gjob redo <job_id> --cascade     # Redo job and all dependent jobs

# Update queued jobs
gjob update <job_id> --gpus 2                    # Update GPU count
gjob update <job_id> --priority 15               # Update priority
gjob update <job_id> --time-limit 04:00:00       # Update time limit
gjob update <job_id> --depends-on 100,101        # Update dependencies
gjob update <job_id> --param batch_size=64       # Update parameter
gjob update <job_id> --gpus 4 --priority 20      # Update multiple fields
```

### Monitoring
```bash
# Watch queue
watch -n 5 gqueue

# Watch running jobs with time limits
watch -n 5 'gqueue -s Running -f JOBID,NAME,TIME,TIMELIMIT'

# Check logs
cat ~/.local/share/gflow/logs/<job_id>.log
tail -f ~/.local/share/gflow/logs/<job_id>.log

# Attach to daemon tmux session
tmux attach -t gflow_server

# Attach to job tmux session
tmux attach -t <job_run_name>
```

## Job States

| Code | Full Name | Description |
|------|-----------|-------------|
| `PD` | Queued | Waiting for resources or dependencies |
| `R` | Running | Currently executing |
| `CD` | Finished | Completed successfully |
| `F` | Failed | Exited with non-zero status |
| `CA` | Cancelled | Manually cancelled by user |
| `TO` | Timeout | Exceeded time limit |

## Time Limit Formats

| Format | Example | Meaning |
|--------|---------|---------|
| `HH:MM:SS` | `2:30:00` | 2 hours, 30 minutes |
| `HH:MM:SS` | `12:00:00` | 12 hours |
| `MM:SS` | `45:30` | 45 minutes, 30 seconds |
| `MM:SS` | `5:00` | 5 minutes |
| `MM` | `30` | 30 minutes |
| `MM` | `120` | 120 minutes (2 hours) |

**Note**: Single number is always minutes, not seconds!

## Output Format Fields

Available fields for `gqueue -f`:
- `JOBID` - Job ID number
- `NAME` - Job run name (tmux session name)
- `ST` - State (short form)
- `TIME` - Elapsed time (HH:MM:SS)
- `TIMELIMIT` - Time limit (HH:MM:SS or UNLIMITED)
- `NODES` - Number of GPUs requested
- `NODELIST(REASON)` - GPU IDs assigned

## Environment Variables

Set by gflow in job environment:
- `CUDA_VISIBLE_DEVICES` - Comma-separated GPU IDs
- `GFLOW_ARRAY_TASK_ID` - Task ID for array jobs (0 for non-array)

## File Locations

```
~/.config/gflow/
  └── gflowd.toml              # Configuration file

~/.local/share/gflow/
  ├── state.json               # Job state (persisted)
  └── logs/
      ├── 1.log                # Job output logs
      ├── 2.log
      └── ...
```

## Common Patterns

### Sequential Jobs (Pipeline)
```bash
# Step 1: Preprocessing
gbatch --time 30 python preprocess.py

# Step 2: Training (depends on step 1)
gbatch --time 4:00:00 --depends-on @ python train.py

# Step 3: Evaluation (depends on step 2)
gbatch --time 10 --depends-on @ python evaluate.py
```

The `@` symbol references the most recently submitted job, making pipelines simple.

### Parallel Jobs (Array)
```bash
# Process 10 tasks in parallel
gbatch --array 1-10 --time 1:00:00 \
       python process.py --task $GFLOW_ARRAY_TASK_ID
```

### GPU Sweeps
```bash
# Try different hyperparameters on different GPUs
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.001
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.01
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.1
```

### Long-Running with Checkpointing
```bash
# Initial training
gbatch --time 8:00:00 --gpus 1 \
       python train.py --checkpoint checkpoint.pth

# Resume if timed out (submit manually after checking)
gbatch --time 8:00:00 --gpus 1 \
       python train.py --resume checkpoint.pth
```

## Tips and Tricks

### 1. Auto-submit on dependency completion
```bash
# Not built-in, but can script it:
while [ "$(gqueue -j $ID1 -f ST)" != "CD" ]; do sleep 5; done
gbatch python next_step.py
```

### 2. Get job output path programmatically
```bash
JOB_ID=42
LOG_PATH="$HOME/.local/share/gflow/logs/${JOB_ID}.log"
tail -f "$LOG_PATH"
```

### 3. Check remaining time (manually)
```bash
# Show time and limit
gqueue -j 42 -f TIME,TIMELIMIT

# Example output:
# TIME         TIMELIMIT
# 01:23:45     02:00:00
# Remaining: ~36 minutes
```

### 4. Filter timed-out jobs
```bash
gqueue -s Timeout -f JOBID,NAME,TIME,TIMELIMIT
```

### 5. Quick job status check
```bash
# Check if job finished successfully
[ "$(gqueue -j 42 -f ST)" = "CD" ] && echo "Success!" || echo "Not done or failed"
```

### 6. Kill all your running jobs
```bash
# Get all running job IDs
RUNNING=$(gqueue -s Running -f JOBID | tail -n +2)
for jobid in $RUNNING; do
    gcancel $jobid
done
```

### 7. Find jobs that timed out
```bash
gqueue -a -s Timeout -f JOBID,NAME,TIME,TIMELIMIT
```

## Troubleshooting

### Job stuck in Queued
```bash
# Check dependencies
gqueue -t

# Check GPU availability
gqueue -s Running -f JOBID,NODES,NODELIST

# Check if dependency finished
gqueue -j <dependency_id> -f ST
```

### Job timed out unexpectedly
```bash
# Check actual runtime
gqueue -j <job_id> -f TIME,TIMELIMIT

# Verify time format (30 = 30 minutes, not seconds!)
# Resubmit with longer limit
gbatch --time 60 ...
```

### Can't find job logs
```bash
# Logs are in
ls -la ~/.local/share/gflow/logs/

# Check job ID is correct
gqueue -a -f JOBID,NAME
```

### Job not receiving GPU
```bash
# Check if GPUs were requested
gqueue -j <job_id> -f JOBID,NODES,NODELIST

# Check GPU availability
nvidia-smi

# Check if other jobs are using GPUs
gqueue -s Running -f JOBID,NODES,NODELIST
```

## Resource Limits

Default scheduler settings:
- **Check interval**: 5 seconds
- **Timeout accuracy**: ±5 seconds
- **Time limit range**: No hard limit
- **Priority range**: 0-255 (default: 10)
- **GPU detection**: Via NVML (NVIDIA GPUs only)

## Exit Codes

Common exit codes in logs:
- `0` - Success
- `1` - General error
- `130` - SIGINT (Ctrl-C / Timeout)
- `137` - SIGKILL (forceful termination)
- `143` - SIGTERM (graceful termination)

## Quick Diagnosis

| Symptom | Likely Cause | Solution |
|---------|--------------|----------|
| Job shows TO | Time limit exceeded | Increase `--time` |
| Job shows F | Script error | Check logs |
| Job stuck PD | Dependency not done | Check dependency state |
| Job stuck PD | No free GPUs | Wait or reduce `--gpus` |
| No output | Pipe-pane issue | Check tmux session |
| Can't attach | Session killed | Job likely finished |

## Best Practices

1. **Always set time limits** for production jobs
2. **Use job arrays** for parallel independent tasks
3. **Implement checkpointing** for long-running jobs
4. **Monitor time usage** with `watch gqueue`
5. **Add buffer** to time estimates (10-20%)
6. **Use dependencies** for pipeline workflows
7. **Check logs** when jobs fail or timeout
8. **Test scripts** with short time limits first

## Getting Help

- Detailed docs: `docs/TIME_LIMITS.md`
- Main README: `README.md`
- Report issues: GitHub Issues
- Source code: GitHub Repository

---

**Quick Help**: Run any command with `--help` for detailed options:
```bash
$ gbatch --help
<!-- cmdrun gbatch --help -->
```

```bash
$ gqueue --help
<!-- cmdrun gqueue --help -->
```

```bash
$ ginfo --help
<!-- cmdrun ginfo --help -->
```
