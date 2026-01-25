# Quick Start

This guide gets you running with gflow in a few minutes.

## 1) Start the Scheduler

Start the daemon (runs inside a tmux session):

```shell
gflowd up
```

Check status:

```shell
gflowd status
```

Verify the client can reach it:

```shell
ginfo
```

## 2) Submit a Job

```shell
gbatch echo 'Hello from gflow!'
```

## 3) Check Queue and Logs

```shell
gqueue
```

Then view output:

```shell
gjob log <job_id>
```

## 4) Stop the Scheduler

```shell
gflowd down
```

## Next Steps

- [Submitting jobs](../user-guide/job-submission)
- [Time limits](../user-guide/time-limits)
- [Job dependencies](../user-guide/job-dependencies)
- [GPU management](../user-guide/gpu-management)
- [Configuration](../user-guide/configuration)
- [Command reference](../reference/quick-reference)

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
