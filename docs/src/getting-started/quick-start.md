# Quick Start

This guide will get you up and running with gflow in 5 minutes.

## Starting the Scheduler

First, start the gflow daemon:

```shell
$ gflowd up
```

This command starts the daemon in a tmux session. If you want to run it in the background, you can use:
```shell
$ gflowd up --detach
```

Run this in its own terminal or tmux session and leave it running. You can confirm that it started successfully with:
```shell
$ gflowd status
```

Example output:
```
Status: Running
The gflowd daemon is running in tmux session 'gflow_server'.
```

Verify it's reachable from another terminal:
```shell
$ ginfo
```

Output will show scheduler status and GPU information.

## Your First Job

Let's submit a simple job:

```shell
$ gbatch echo 'Hello from gflow!'
```

This will submit a job that prints "Hello from gflow!" to the log.

## Checking Job Status

View the job queue:

```shell
$ gqueue
```

Example output:
```
JOBID   NAME            ST   TIME       NODES   NODELIST(REASON)
1       gflow-job-1     CD   00:00:00   0       -
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

```shell
$ sleep 6
$ gjob log 1
```

## Submitting Jobs with Options

### Job with GPU Request

```shell
gbatch --gpus 1 nvidia-smi
```

### Job with Time Limit

```shell
# 30-minute limit
gbatch --time 30 python train.py

# 2-hour limit
gbatch --time 2:00:00 python long_train.py
```

### Job with Priority

```shell
# Higher priority (runs first)
gbatch --priority 100 python urgent_task.py

# Lower priority (default is 10)
gbatch --priority 5 python background_task.py
```

### Job Script

Create a file `my_job.sh`:
```shell
#!/bin/shell
# GFLOW --gpus 1
# GFLOW --time 1:00:00
# GFLOW --priority 20

echo "Job started at $(date)"
python train.py --epochs 10
echo "Job finished at $(date)"
```

Make it executable and submit:
```shell
chmod +x my_job.sh
gbatch my_job.sh
```

## Job Dependencies

Run jobs in sequence using the @ syntax:

```shell
# Job 1: Preprocessing
gbatch --name "prep" python preprocess.py

# Job 2: Training (depends on job 1)
gbatch --depends-on @ --name "train" python train.py

# Job 3: Evaluation (depends on job 2)
gbatch --depends-on @ --name "eval" python evaluate.py
```

The `@` symbol always references the most recently submitted job, making it easy to chain dependencies.

View dependency tree:
```shell
gqueue -t
```

## Monitoring Jobs

### Watch Queue in Real-time

```shell
watch -n 2 gqueue
```

### Filter by State

```shell
# Show only running jobs
gqueue -s Running

# Show running and queued jobs
gqueue -s Running,Queued
```

### Custom Output Format

```shell
$ gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
```

Example output:
```
JOBID   NAME            ST   TIME       TIMELIMIT
1       gflow-job-1     CD   00:00:05   00:01:00
2       gflow-job-2     R    00:00:10   UNLIMITED
```

### View Specific Jobs

```shell
# Single job
gqueue -j 5

# Multiple jobs
gqueue -j 5,6,7
```

## Cancelling Jobs

Cancel a job:

```shell
gcancel 5
```

Output:
```
Job 5 cancelled.
```

## Attaching to Running Jobs

Each job runs in a tmux session. You can attach to see live output:

```shell
# Get the job's session name from gqueue
gqueue -f JOBID,NAME

# Attach to the session
gjob attach -t <job_id>

# Detach without stopping the job
# Press: Ctrl+B then D
```

## Stopping the Scheduler

When you're done:

```shell
gflowd down
```

This stops the daemon, saves state, and removes the tmux session.

## Example Workflow

Here's a complete example workflow:

```shell
# 1. Start scheduler
gflowd up

# 2. Submit preprocessing job
gbatch --time 10 --name prep python preprocess.py

# 3. Submit training jobs that depend on preprocessing
gbatch --time 2:00:00 --gpus 1 --depends-on @ --name train_lr001 python train.py --lr 0.001

gbatch --time 2:00:00 --gpus 1 --depends-on @~1 --name train_lr01 python train.py --lr 0.01

# Note: Both training jobs depend on @~1 (the prep job), skipping each other

# 4. Monitor jobs
watch gqueue -t

# 5. Check logs when done
gjob log 1
gjob log 2
gjob log 3

# 6. Stop scheduler
gflowd down
```

**@ Syntax Explained**:
- `@` - Most recently submitted job
- `@~1` - Second most recently submitted job
- `@~2` - Third most recently submitted job

This makes it easy to create complex workflows without manually tracking job IDs!

## Common Patterns

### Parallel Jobs (Array)

Run multiple similar tasks:

```shell
gbatch --array 1-10 --time 30 \
       python process.py --task $GFLOW_ARRAY_TASK_ID
```

This creates 10 jobs, each with `$GFLOW_ARRAY_TASK_ID` set to 1, 2, ..., 10.

### GPU Sweeps

Test different hyperparameters on different GPUs:

```shell
# Each job gets 1 GPU
gbatch --gpus 1 --time 4:00:00 python train.py --lr 0.001
gbatch --gpus 1 --time 4:00:00 python train.py --lr 0.01
gbatch --gpus 1 --time 4:00:00 python train.py --lr 0.1
```

### Conda Environment

Use a specific conda environment:

```shell
gbatch --conda-env myenv python script.py
```

## Tips for Beginners

1. **Always set time limits** for production jobs:
   ```shell
   gbatch --time 2:00:00 your_command
   ```

2. **Use `watch gqueue`** to monitor jobs in real-time

3. **Check logs** when jobs fail:
   ```shell
   cat ~/.local/share/gflow/logs/<job_id>.log
   ```

4. **Test scripts first** with short time limits:
   ```shell
   gbatch --time 1 shell test.sh
   ```

5. **Use job dependencies** for workflows:
   ```shell
   gbatch --depends-on <prev_job_id> your_command
   ```

## Next Steps

Now that you're familiar with the basics, explore:

- [Job Submission](../user-guide/job-submission) - Detailed job options
- [Time Limits](../user-guide/time-limits) - Managing job timeouts
- [Job Dependencies](../user-guide/job-dependencies) - Complex workflows
- [GPU Management](../user-guide/gpu-management) - GPU allocation
- [Quick Reference](../reference/quick-reference) - Command cheat sheet

---

**Previous**: [Installation](./installation) | **Next**: [Job Submission](../user-guide/job-submission)
