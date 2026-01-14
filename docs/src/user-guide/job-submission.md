# Job Submission

This guide covers all aspects of submitting jobs with `gbatch`, from basic usage to advanced features.

## Overview

`gbatch` is gflow's job submission tool, similar to Slurm's `sbatch`. It supports both direct command execution and script-based job submission.

## Basic Usage

### Submitting a Command

The simplest way to submit a job is to provide the command directly:

```bash
gbatch python script.py
```

Output:
```
Submitted batch job 1 (silent-pump-6338)
```

**No quotes needed** for simple commands! Arguments are automatically joined:

```bash
gbatch python train.py --epochs 100 --lr 0.01
```

### Command Argument Safety

gflow automatically handles special characters in command arguments using shell escaping:

```bash
# Arguments with spaces
gbatch python script.py --message "Hello World"

# Arguments with special characters
gbatch python script.py --pattern 'test_*.py'

# Complex arguments
gbatch bash -c 'echo $USER && python script.py'
```

**How it works**:
- Command arguments are properly escaped before execution
- Prevents shell injection and unintended command interpretation
- Special characters like spaces, quotes, and wildcards are handled safely
- Uses the `shell-escape` library to ensure safety

**Best practice**: While gflow handles escaping automatically, it's still recommended to:
- Test complex commands locally first
- Use explicit quoting for clarity
- Avoid overly complex inline commands (use script files instead)

### Submitting a Script

Create a script file and submit it:

```bash
# Create script
cat > my_job.sh << 'EOF'
#!/bin/bash
echo "Hello from gflow!"
python train.py
EOF

# Make executable
chmod +x my_job.sh

# Submit
gbatch my_job.sh
```

## Resource Allocation

### GPU Requests

Request GPUs for your job:

```bash
# Request 1 GPU
gbatch --gpus 1 python train.py

# Request 2 GPUs
gbatch --gpus 2 python multi_gpu_train.py
```

The scheduler sets `CUDA_VISIBLE_DEVICES` automatically to the allocated GPUs.

**Check GPU allocation**:
```bash
$ gqueue -f JOBID,NAME,NODES,NODELIST
JOBID    NAME                NODES    NODELIST(REASON)
42       silent-pump-6338    1        0
43       brave-river-1234    2        1,2
```

### Conda Environment

Activate a conda environment before running your job:

```bash
gbatch --conda-env myenv python script.py
```

This is equivalent to running:
```bash
conda activate myenv
python script.py
```

## Job Scheduling Options

### Priority

Control when your job runs relative to others:

```bash
# High priority (runs first)
gbatch --priority 100 python urgent.py

# Default priority
gbatch python normal.py  # priority = 10

# Low priority (runs last)
gbatch --priority 1 python background.py
```

**Priority details**:
- Range: 0-255
- Default: 10
- Higher values = higher priority
- Jobs are scheduled based on a multi-factor priority system (see below)

**Scheduling Priority Hierarchy**:

When resources become available, gflow schedules jobs using a three-level priority system:

1. **User Priority** (Primary): Jobs with higher `--priority` values run first
2. **Time Limit Bonus** (Secondary): Among jobs with equal priority:
   - Time-limited jobs are preferred over unlimited jobs
   - Shorter jobs run before longer jobs
3. **Submission Order** (Tertiary): Jobs submitted earlier run first (FIFO)

**Examples**:

```bash
# These jobs will run in the following order when GPUs become available:

# 1st: High priority, even though unlimited
gbatch --priority 20 python urgent.py

# 2nd: Same priority, but 10-minute limit beats unlimited
gbatch --priority 10 --time 10 python quick.py

# 3rd: Same priority, but 1-hour limit (submitted first)
gbatch --priority 10 --time 1:00:00 python train1.py  # Job ID 100

# 4th: Same priority and limit, but submitted later
gbatch --priority 10 --time 1:00:00 python train2.py  # Job ID 101

# 5th: Same priority, unlimited (submitted first)
gbatch --priority 10 python long1.py  # Job ID 102

# 6th: Same priority, unlimited (submitted later)
gbatch --priority 10 python long2.py  # Job ID 103
```

**Key Insights**:
- Setting `--time` not only prevents runaway jobs but also improves scheduling priority
- Shorter time limits get slight preference, encouraging accurate estimates
- Submission order acts as a fair tie-breaker when all else is equal

### Time Limits

Set maximum runtime for jobs:

```bash
# 30 minutes
gbatch --time 30 python quick.py

# 2 hours
gbatch --time 2:00:00 python train.py

# 5 minutes 30 seconds
gbatch --time 5:30 python test.py
```

See [Time Limits](./time-limits) for comprehensive documentation.

### Job Names

By default, jobs get auto-generated names (e.g., "silent-pump-6338"). You can specify custom names:

```bash
gbatch --name "my-training-run" python train.py
```

**Note**: The `--name` option is for custom naming. If not specified, a random name is generated.

## Job Dependencies

Make jobs wait for other jobs to complete:

```bash
# Job 1: Preprocessing
gbatch --name "prep" python preprocess.py
# Returns: Submitted batch job 1

# Job 2: Training (waits for job 1)
gbatch --depends-on 1 --name "train" python train.py

# Job 3: Evaluation (waits for job 2)
gbatch --depends-on 2 --name "eval" python evaluate.py
```

See [Job Dependencies](./job-dependencies) for advanced dependency management.

## Updating Queued Jobs

You can update parameters for jobs that are still queued or on hold using the `gjob update` command. This is useful for fixing mistakes, adjusting resource requirements, or changing priorities before a job starts running.

### When Updates Are Allowed

- **Queued jobs**: All parameters can be updated
- **Held jobs**: All parameters can be updated
- **Running jobs**: Cannot be updated (job is already executing)
- **Completed jobs**: Cannot be updated (job has finished)

### What Can Be Updated

You can update the following job parameters:

- Command or script
- GPU requirements
- Conda environment
- Priority
- Time and memory limits
- Dependencies
- Template parameters
- Max concurrent jobs in group

### Basic Usage

```bash
# Update GPU count for job 123
gjob update 123 --gpus 2

# Update priority
gjob update 123 --priority 15

# Update multiple parameters at once
gjob update 123 --gpus 4 --priority 20 --time-limit 02:00:00

# Update multiple jobs
gjob update 123,124,125 --priority 10
```

### Updating Resources

```bash
# Change GPU allocation
gjob update 123 --gpus 4

# Update time limit
gjob update 123 --time-limit 04:30:00

# Update memory limit
gjob update 123 --memory-limit 32G

# Clear time limit (no limit)
gjob update 123 --clear-time-limit

# Clear memory limit
gjob update 123 --clear-memory-limit
```

### Updating Dependencies

```bash
# Update dependencies (AND logic - all must finish)
gjob update 123 --depends-on 100,101,102

# Update with explicit AND logic
gjob update 123 --depends-on-all 100,101,102

# Update with OR logic (any one must finish)
gjob update 123 --depends-on-any 100,101

# Enable auto-cancel when dependency fails
gjob update 123 --auto-cancel-on-dep-failure

# Disable auto-cancel
gjob update 123 --no-auto-cancel-on-dep-failure
```

### Updating Command or Script

```bash
# Update command
gjob update 123 --command "python train.py --epochs 100"

# Update script path
gjob update 123 --script /path/to/new_script.sh

# Update conda environment
gjob update 123 --conda-env myenv

# Clear conda environment
gjob update 123 --clear-conda-env
```

### Updating Template Parameters

```bash
# Update single parameter
gjob update 123 --param learning_rate=0.001

# Update multiple parameters
gjob update 123 --param batch_size=64 --param epochs=100
```

### Common Use Cases

**Fixing a mistake in submitted job**:
```bash
# Oops, submitted with wrong GPU count
gbatch --gpus 1 python train.py
# Returns: Submitted batch job 123

# Fix it before it runs
gjob update 123 --gpus 4
```

**Adjusting to changing conditions**:
```bash
# More GPUs became available, increase allocation
gjob update 123 --gpus 8

# System is busy, lower priority to let urgent jobs run first
gjob update 123 --priority 5
```

**Iterative refinement**:
```bash
# Start with conservative estimate
gbatch --time-limit 01:00:00 python experiment.py
# Returns: Submitted batch job 123

# Realize it needs more time
gjob update 123 --time-limit 04:00:00
```

### Limitations and Restrictions

- Cannot update job ID, submitted by, or group ID (immutable)
- Cannot update running or completed jobs
- Dependency updates must not create circular dependencies
- Priority must be between 0-255
- All dependency job IDs must exist

### Error Handling

If an update fails, you'll see a clear error message:

```bash
$ gjob update 123 --gpus 4
Error updating job 123: Job 123 is in state 'Running' and cannot be updated.
Only queued or held jobs can be updated.

$ gjob update 123 --depends-on 123
Error updating job 123: Circular dependency detected: Job 123 depends on Job 123,
which has a path back to Job 123
```

## Job Arrays

Run multiple similar tasks in parallel:

```bash
# Create 10 jobs with task IDs 1-10
gbatch --array 1-10 python process.py --task '$GFLOW_ARRAY_TASK_ID'
```

**How it works**:
- Creates 10 separate jobs
- Each job has `$GFLOW_ARRAY_TASK_ID` set to its task number
- All jobs share the same resource requirements
- Useful for parameter sweeps, data processing, etc.

**Example with different parameters**:
```bash
gbatch --array 1-5 --gpus 1 --time 2:00:00 \
       python train.py --lr '$(echo "0.001 0.01 0.1 0.5 1.0" | cut -d" " -f$GFLOW_ARRAY_TASK_ID)'
```

**Environment variable**:
- `GFLOW_ARRAY_TASK_ID`: Task ID for array jobs (1, 2, 3, ...)
- Set to 0 for non-array jobs

## Script Directives

Instead of command-line options, you can embed job requirements in your script using `# GFLOW` directives:

```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 2:00:00
# GFLOW --priority 20
# GFLOW --conda-env myenv

echo "Starting training..."
python train.py --epochs 100
echo "Training complete!"
```

Submit the script:
```bash
gbatch my_script.sh
```

**Directive precedence**:
- Command-line arguments override script directives
- Example: `gbatch --time 1:00:00 my_script.sh` overrides the `--time` directive in the script

**Supported directives**:
- `# GFLOW --gpus <N>`
- `# GFLOW --time <TIME>`
- `# GFLOW --priority <N>`
- `# GFLOW --conda-env <ENV>`
- `# GFLOW --depends-on <ID>`

## Creating Script Templates

Use `gbatch new` to create a job script template:

```bash
$ gbatch new my_job
```

This creates `my_job.sh` with a template:
```bash
#!/bin/bash
# GFLOW --gpus 0
# GFLOW --time 1:00:00
# GFLOW --priority 10

# Your commands here
echo "Job started at $(date)"

# Add your actual commands
# python script.py

echo "Job finished at $(date)"
```

Edit the template and submit:
```bash
# Edit the script
vim my_job.sh

# Make executable
chmod +x my_job.sh

# Submit
gbatch my_job.sh
```

### Automatic Template Generation

The job script template is **automatically generated** from the `gbatch` CLI definition to ensure it always reflects available options:

- **Template Source**: The template is generated from `src/bin/gbatch/cli.rs`
- **Automatic Sync**: A pre-commit hook automatically regenerates the template when command-line options change
- **Always Current**: You always get the latest available options in your templates

**For developers**: See `scripts/README.md` for details on how the template generation works.

## Environment Variables

gflow automatically sets these environment variables in your job:

| Variable | Description | Example |
|----------|-------------|---------|
| `CUDA_VISIBLE_DEVICES` | GPU IDs allocated to the job | `0,1` |
| `GFLOW_ARRAY_TASK_ID` | Task ID for array jobs (0 for non-array) | `5` |

**Example usage**:
```bash
#!/bin/bash
echo "Using GPUs: $CUDA_VISIBLE_DEVICES"
echo "Array task ID: $GFLOW_ARRAY_TASK_ID"
python train.py
```

## Output and Logging

Job output is automatically captured to log files:

**Log location**: `~/.local/share/gflow/logs/<job_id>.log`

**View logs**:
```bash
# View completed job log
cat ~/.local/share/gflow/logs/42.log

# Follow running job log
tail -f ~/.local/share/gflow/logs/42.log
```

**Attach to running job** (via tmux):
```bash
# Get job session name
gqueue -f JOBID,NAME

# Attach to session
tmux attach -t <session_name>

# Detach without stopping (Ctrl-B, then D)
```

## Advanced Examples

### Parameter Sweep

Test multiple hyperparameters:

```bash
# Submit multiple training runs
for lr in 0.001 0.01 0.1; do
    gbatch --gpus 1 --time 4:00:00 \
           --name "train-lr-$lr" \
           python train.py --lr $lr
done
```

### Pipeline with Dependencies

```bash
# Step 1: Data preprocessing
gbatch --time 30 python preprocess.py

# Step 2: Training
gbatch --time 4:00:00 --gpus 1 --depends-on @ python train.py

# Step 3: Evaluation
gbatch --time 10 --depends-on @ python evaluate.py
```

The `@` symbol references the most recently submitted job, making pipelines simple and clean.

### Multi-stage Job Script

```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 8:00:00

set -e  # Exit on error

echo "Stage 1: Data preparation"
python prepare_data.py

echo "Stage 2: Model training"
python train.py --checkpoint model.pth

echo "Stage 3: Evaluation"
python evaluate.py --model model.pth

echo "All stages complete!"
```

### Conditional Job Submission

```bash
#!/bin/bash
# Submit job only if previous job succeeded

PREV_JOB=42
STATUS=$(gqueue -j $PREV_JOB -f ST | tail -n 1)

if [ "$STATUS" = "CD" ]; then
    gbatch python next_step.py
else
    echo "Previous job not completed successfully"
fi
```

## Common Patterns

### Long-running with Checkpointing

```python
# train.py with checkpoint support
import signal
import sys

def save_checkpoint():
    print("Saving checkpoint...")
    # Save model state
    torch.save(model.state_dict(), 'checkpoint.pth')

def signal_handler(sig, frame):
    save_checkpoint()
    sys.exit(0)

signal.signal(signal.SIGINT, signal_handler)

# Training loop
for epoch in range(epochs):
    train_epoch()
    if epoch % 10 == 0:
        save_checkpoint()
```

Submit with time limit:
```bash
gbatch --time 8:00:00 --gpus 1 python train.py
```

### GPU Utilization Check

```bash
#!/bin/bash
# GFLOW --gpus 1

echo "Allocated GPUs: $CUDA_VISIBLE_DEVICES"
nvidia-smi --query-gpu=index,name,memory.total --format=csv
python train.py
```

## Validation and Error Handling

`gbatch` validates your submission before accepting it:

**Common validation errors**:
- **Invalid dependency**: Job ID doesn't exist
  ```
  Error: Dependency job 999 not found
  ```

- **Circular dependency**: Job depends on itself or creates a cycle
  ```
  Error: Circular dependency detected
  ```

- **Invalid time format**: Malformed time specification
  ```
  Error: Invalid time format. Use HH:MM:SS, MM:SS, or MM
  ```

- **Script not found**: File doesn't exist
  ```
  Error: Script file not found: missing.sh
  ```

## Tips and Best Practices

1. **Always set time limits** for production jobs to prevent runaway processes
2. **Use meaningful names** for easier job tracking
3. **Test scripts locally** before submitting
4. **Add error handling** (`set -e`) in bash scripts
5. **Implement checkpointing** for long-running jobs
6. **Use job arrays** for parallel independent tasks
7. **Check dependencies** before submitting dependent jobs
8. **Monitor GPU usage** when requesting multiple GPUs
9. **Use conda environments** for reproducibility
10. **Add logging** to your scripts for easier debugging

## Troubleshooting

### Issue: Job submission fails with "dependency not found"

**Solution**: Verify the dependency job exists:
```bash
gqueue -j <dependency_id>
```

### Issue: Job doesn't get GPU

**Check**:
1. Did you request GPU? `--gpus 1`
2. Are GPUs available? `ginfo`
3. Are other jobs using all GPUs? `gqueue -s Running -f NODES,NODELIST`

### Issue: Conda environment not activating

**Check**:
1. Environment name is correct: `conda env list`
2. Conda is initialized in your shell
3. Check job logs for activation errors

### Issue: Script not executable

**Solution**:
```bash
chmod +x my_script.sh
gbatch my_script.sh
```

## Reference

**Full command syntax**:
```bash
gbatch [OPTIONS] <SCRIPT>
gbatch [OPTIONS] <COMMAND> [ARGS...]
```

**All options**:
- `--gpus <N>` or `-g <N>`: Number of GPUs
- `--time <TIME>` or `-t <TIME>`: Time limit
- `--priority <N>`: Job priority (0-255, default: 10)
- `--depends-on <ID>`: Job dependency
- `--conda-env <ENV>` or `-c <ENV>`: Conda environment
- `--array <SPEC>`: Job array (e.g., "1-10")
- `--name <NAME>`: Custom job name
- `--config <PATH>`: Custom config file (hidden)

**Get help**:
```bash
$ gbatch --help
<!-- cmdrun gbatch --help -->
```

## See Also

- [Time Limits](./time-limits) - Detailed time limit documentation
- [Job Dependencies](./job-dependencies) - Advanced dependency workflows
- [GPU Management](./gpu-management) - GPU allocation and monitoring
- [Quick Reference](../reference/quick-reference) - Command cheat sheet
