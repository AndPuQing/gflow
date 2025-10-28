# Quick Start

This guide will get you up and running with gflow in 5 minutes.

## Starting the Scheduler

First, start the gflow daemon:

```bash
gctl start
```

You should see:
```
gflowd started.
```

Verify it's running:
```bash
gctl status
```

Expected output:
```
Status: Running
The gflowd daemon is running in tmux session 'gflow_server'.
```

## Your First Job

Let's submit a simple job:

```bash
gbatch --command "echo 'Hello from gflow!'; sleep 5; echo 'Job complete!'"
```

Output:
```
Submitted batch job 1 (silent-pump-6338)
```

## Checking Job Status

View the job queue:

```bash
gqueue
```

Output:
```
JOBID    NAME                 ST    TIME         NODES    NODELIST(REASON)
1        silent-pump-6338     R     00:00:02     0        -
```

Job states:
- `PD` (Queued) - Waiting to run
- `R` (Running) - Currently executing
- `CD` (Finished) - Completed successfully
- `F` (Failed) - Failed with error
- `CA` (Cancelled) - Manually cancelled
- `TO` (Timeout) - Exceeded time limit

## Viewing Job Output

Job output is automatically logged:

```bash
# Wait for job to complete
sleep 6

# View the log
cat ~/.local/share/gflow/logs/1.log
```

You should see:
```
Hello from gflow!
Job complete!
```

## Submitting Jobs with Options

### Job with GPU Request

```bash
gbatch --gpus 1 --command "nvidia-smi"
```

### Job with Time Limit

```bash
# 30-minute limit
gbatch --time 30 --command "python train.py"

# 2-hour limit
gbatch --time 2:00:00 --command "python long_train.py"
```

### Job with Priority

```bash
# Higher priority (runs first)
gbatch --priority 100 --command "python urgent_task.py"

# Lower priority (default is 10)
gbatch --priority 5 --command "python background_task.py"
```

### Job Script

Create a file `my_job.sh`:
```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 1:00:00
# GFLOW --priority 20

echo "Job started at $(date)"
python train.py --epochs 10
echo "Job finished at $(date)"
```

Make it executable and submit:
```bash
chmod +x my_job.sh
gbatch my_job.sh
```

## Job Dependencies

Run jobs in sequence:

```bash
# Job 1: Preprocessing
gbatch --command "python preprocess.py" --name "prep"
# Note the job ID, e.g., 2

# Job 2: Training (depends on job 2)
gbatch --command "python train.py" --depends-on 2 --name "train"

# Job 3: Evaluation (depends on job 3)
gbatch --command "python evaluate.py" --depends-on 3 --name "eval"
```

View dependency tree:
```bash
gqueue -t
```

## Monitoring Jobs

### Watch Queue in Real-time

```bash
watch -n 2 gqueue
```

### Filter by State

```bash
# Show only running jobs
gqueue -s Running

# Show running and queued jobs
gqueue -s Running,Queued
```

### Custom Output Format

```bash
# Show job ID, name, state, time, and time limit
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
```

### View Specific Jobs

```bash
# Single job
gqueue -j 5

# Multiple jobs
gqueue -j 5,6,7
```

## Cancelling Jobs

Cancel a job:

```bash
gcancel 5
```

Output:
```
Job 5 cancelled.
```

## Attaching to Running Jobs

Each job runs in a tmux session. You can attach to see live output:

```bash
# Get the job's session name from gqueue
gqueue -f JOBID,NAME

# Attach to the session
tmux attach -t <session_name>

# Detach without stopping the job
# Press: Ctrl+B then D
```

## Stopping the Scheduler

When you're done:

```bash
gctl stop
```

This will:
- Stop the scheduler daemon
- Keep job state persistent
- Preserve logs
- Running jobs will be marked as failed (they're in tmux sessions, so they actually stop)

## Example Workflow

Here's a complete example workflow:

```bash
# 1. Start scheduler
gctl start

# 2. Submit preprocessing job
gbatch --time 10 --command "python preprocess.py --output data.pkl" --name prep
# Job ID: 1

# 3. Submit training jobs (depend on preprocessing)
gbatch --time 2:00:00 --gpus 1 --depends-on 1 --command "python train.py --lr 0.001" --name train_lr001
gbatch --time 2:00:00 --gpus 1 --depends-on 1 --command "python train.py --lr 0.01" --name train_lr01

# 4. Monitor jobs
watch gqueue

# 5. Check logs when done
cat ~/.local/share/gflow/logs/1.log
cat ~/.local/share/gflow/logs/2.log
cat ~/.local/share/gflow/logs/3.log

# 6. Stop scheduler
gctl stop
```

## Common Patterns

### Parallel Jobs (Array)

Run multiple similar tasks:

```bash
gbatch --array 1-10 --time 30 \
       --command 'python process.py --task $GFLOW_ARRAY_TASK_ID'
```

This creates 10 jobs, each with `$GFLOW_ARRAY_TASK_ID` set to 1, 2, ..., 10.

### GPU Sweeps

Test different hyperparameters on different GPUs:

```bash
# Each job gets 1 GPU
gbatch --gpus 1 --time 4:00:00 --command "python train.py --lr 0.001"
gbatch --gpus 1 --time 4:00:00 --command "python train.py --lr 0.01"
gbatch --gpus 1 --time 4:00:00 --command "python train.py --lr 0.1"
```

### Conda Environment

Use a specific conda environment:

```bash
gbatch --conda-env myenv --command "python script.py"
```

## Tips for Beginners

1. **Always set time limits** for production jobs:
   ```bash
   gbatch --time 2:00:00 --command "..."
   ```

2. **Use `watch gqueue`** to monitor jobs in real-time

3. **Check logs** when jobs fail:
   ```bash
   cat ~/.local/share/gflow/logs/<job_id>.log
   ```

4. **Test scripts first** with short time limits:
   ```bash
   gbatch --time 1 --command "bash test.sh"
   ```

5. **Use job dependencies** for workflows:
   ```bash
   gbatch --depends-on <prev_job_id> --command "..."
   ```

## Next Steps

Now that you're familiar with the basics, explore:

- [Job Submission](../user-guide/job-submission.md) - Detailed job options
- [Time Limits](../user-guide/time-limits.md) - Managing job timeouts
- [Job Dependencies](../user-guide/job-dependencies.md) - Complex workflows
- [GPU Management](../user-guide/gpu-management.md) - GPU allocation
- [Quick Reference](../reference/quick-reference.md) - Command cheat sheet

---

**Previous**: [Installation](./installation.md) | **Next**: [Job Submission](../user-guide/job-submission.md)
