# gbatch Command Reference

Complete reference for the `gbatch` command - gflow's job submission tool.

## Synopsis

```bash
gbatch [OPTIONS] [COMMAND...]
gbatch [OPTIONS] [SCRIPT]
gbatch new <NAME>
```

## Description

`gbatch` submits jobs to the gflow scheduler. It supports both script-based and direct command execution using positional arguments, with extensive options for resource allocation, scheduling, and job dependencies.

## Modes

### Script Submission

Submit a script file:

```bash
gbatch [OPTIONS] <SCRIPT>
```

The script can contain `# GFLOW` directives for job parameters.

### Direct Command

Execute a command directly using positional arguments:

```bash
gbatch [OPTIONS] <COMMAND> [ARGS...]
```

The command and its arguments come after all options. Command-line options take precedence over script directives.

### Template Creation

Create a new job script template:

```bash
gbatch new <NAME>
```

Creates `<NAME>.sh` with a template structure.

## Options

### Job Submission

#### `<SCRIPT>`

Path to script file to execute.

**Example**:
```bash
gbatch my_job.sh
gbatch ./scripts/train.sh
gbatch /absolute/path/to/job.sh
```

**Requirements**:
- File must exist
- Should be executable (`chmod +x`)
- Can contain `# GFLOW` directives

**Mutually exclusive** with direct command execution.

#### Direct Command Execution

When no script is provided, all arguments after the options are treated as the command to execute.

**Example**:
```bash
gbatch python train.py
gbatch echo 'Hello'; sleep 10
gbatch nvidia-smi
```

**Notes**:
- Executed in bash shell
- Can contain multiple commands (use `;` or `&&`)
- Mutually exclusive with `<SCRIPT>`
- Commands with complex arguments should be quoted appropriately

### Resource Allocation

#### `--gpus <N>`, `-g <N>`

Number of GPUs to request.

**Example**:
```bash
# Request 1 GPU
gbatch --gpus 1 python train.py

# Request 2 GPUs
gbatch -g 2 python multi_gpu_train.py

# No GPU (default)
gbatch python cpu_task.py
```

**Behavior**:
- Default: 0 (no GPU)
- Sets `CUDA_VISIBLE_DEVICES` automatically
- Job waits if insufficient GPUs available
- See [GPU Management](../user-guide/gpu-management)

#### `--conda-env <ENV>`

Conda environment to activate.

**Example**:
```bash
gbatch --conda-env myenv python script.py
gbatch --conda-env pytorch_env my_training.sh
```

**Behavior**:
- Runs `conda activate <ENV>` before job
- Requires conda to be initialized in shell
- Job fails if environment doesn't exist

**Auto-detection** (for direct command mode only):
- If `--conda-env` is not specified, gflow automatically detects your currently active conda environment
- Uses the `CONDA_DEFAULT_ENV` environment variable
- Only applies when using direct command execution, not when submitting scripts
- Explicit `--conda-env` always takes precedence over auto-detection

```bash
# With auto-detection (uses currently active conda env)
conda activate myenv
gbatch python script.py  # Will use 'myenv'

# Explicit override (ignores active env)
conda activate myenv
gbatch --conda-env otherenv python script.py  # Uses 'otherenv'

# No conda environment
conda deactivate
gbatch python script.py  # No conda activation
```

### Scheduling Options

#### `--priority <N>`

Job priority (0-255, default: 10).

**Example**:
```bash
# High priority (runs first)
gbatch --priority 100 python urgent.py

# Default priority
gbatch python normal.py  # priority = 10

# Low priority (runs last)
gbatch --priority 1 python background.py
```

**Behavior**:
- Higher values = higher priority
- Jobs with higher priority start first when resources free
- Doesn't preempt running jobs

#### `--depends-on <ID>`

Single job dependency. Job waits for the specified job to finish successfully.

**Example**:
```bash
# Job 1
gbatch python preprocess.py
# Returns job ID 1

# Job 2 (depends on 1)
gbatch --depends-on 1 python train.py

# Equivalent shorthand using the last submission
gbatch --depends-on @ python train.py

# Two submissions back
gbatch --depends-on @~2 python evaluate.py
```

**Behavior**:
- Job waits for dependency to reach `Finished` state
- If dependency fails, this job is auto-cancelled by default (see `--no-auto-cancel`)
- See [Job Dependencies](../user-guide/job-dependencies)
- Shorthand values resolve to the most recent job IDs recorded by `gbatch`

**Validation**:
- Dependency job must exist
- No circular dependencies allowed
- Cannot depend on self
- `@~N` requires at least `N` previous submissions

**Conflicts with**: `--depends-on-all`, `--depends-on-any`

#### `--depends-on-all <IDs>`

Multiple job dependencies with AND logic. Job waits for **all** specified jobs to finish successfully.

**Example**:
```bash
# Run three preprocessing jobs
gbatch python preprocess_part1.py  # Job 101
gbatch python preprocess_part2.py  # Job 102
gbatch python preprocess_part3.py  # Job 103

# Training waits for ALL preprocessing jobs
gbatch --depends-on-all 101,102,103 --gpus 2 python train.py

# Using @ syntax
gbatch --depends-on-all @,@~1,@~2 --gpus 2 python train.py
```

**Behavior**:
- Job waits for **all** dependencies to reach `Finished` state
- If any dependency fails, this job is auto-cancelled by default
- Accepts comma-separated job IDs or shorthands
- See [Job Dependencies](../user-guide/job-dependencies)

**Conflicts with**: `--depends-on`, `--depends-on-any`

#### `--depends-on-any <IDs>`

Multiple job dependencies with OR logic. Job starts when **any one** specified job finishes successfully.

**Example**:
```bash
# Try multiple data sources in parallel
gbatch python fetch_from_source_a.py  # Job 201
gbatch python fetch_from_source_b.py  # Job 202
gbatch python fetch_from_source_c.py  # Job 203

# Process whichever source succeeds first
gbatch --depends-on-any 201,202,203 python process_data.py

# Using @ syntax
gbatch --depends-on-any @,@~1,@~2 python process_data.py
```

**Behavior**:
- Job starts when **any one** dependency reaches `Finished` state
- If all dependencies fail, this job is auto-cancelled by default
- Accepts comma-separated job IDs or shorthands
- Useful for fallback scenarios
- See [Job Dependencies](../user-guide/job-dependencies)

**Conflicts with**: `--depends-on`, `--depends-on-all`

#### `--no-auto-cancel`

Disable automatic cancellation when dependencies fail.

**Example**:
```bash
# Job will remain queued if dependency fails
gbatch --depends-on 1 --no-auto-cancel python train.py
```

**Behavior**:
- By default, jobs are auto-cancelled when dependencies fail
- With this flag, jobs remain in `Queued` state indefinitely
- You must manually cancel with `gcancel` if dependencies fail
- Works with `--depends-on`, `--depends-on-all`, and `--depends-on-any`

#### `--time <TIME>`, `-t <TIME>`

Maximum job runtime (time limit).

**Formats**:
- `MM`: Minutes only (e.g., `30` = 30 minutes)
- `MM:SS`: Minutes and seconds (e.g., `5:30` = 5min 30sec)
- `HH:MM:SS`: Hours, minutes, seconds (e.g., `2:00:00` = 2 hours)

**Example**:
```bash
# 30 minutes
gbatch --time 30 python quick.py

# 2 hours
gbatch --time 2:00:00 python train.py

# 5 minutes 30 seconds
gbatch -t 5:30 python test.py
```

**Behavior**:
- Job terminated if exceeds limit
- Graceful termination (SIGINT/Ctrl-C)
- State changes to `Timeout`
- See [Time Limits](../user-guide/time-limits)

#### `--array <SPEC>`

Create job array.

**Format**: `START-END` (e.g., `1-10`)

**Example**:
```bash
# Create 10 jobs (tasks 1-10)
gbatch --array 1-10 python process.py --task $GFLOW_ARRAY_TASK_ID

# Create 5 jobs with GPUs
gbatch --array 1-5 --gpus 1 python train.py --fold $GFLOW_ARRAY_TASK_ID
```

**Behavior**:
- Creates multiple independent jobs
- Each job gets unique `$GFLOW_ARRAY_TASK_ID`
- All jobs share same resource requirements
- Useful for parameter sweeps

**Environment variable**:
- `GFLOW_ARRAY_TASK_ID`: Task number (1, 2, 3, ...)
- Set to 0 for non-array jobs

#### `--name <NAME>`

Custom job name.

**Example**:
```bash
gbatch --name "my-training-run" python train.py
gbatch --name "experiment-1" my_job.sh
```

**Behavior**:
- Default: Auto-generated name (e.g., "silent-pump-6338")
- Used as tmux session name
- Helps identify jobs in queue
- Must be unique (or gflow appends suffix)

### Global Options

#### `--config <PATH>`

Use custom configuration file (hidden option).

**Example**:
```bash
gbatch --config /path/to/custom.toml your_command
```

#### `--help`, `-h`

Display help message.

```bash
$ gbatch --help
<!-- cmdrun gbatch --help -->
```

#### `--version`, `-V`

Display version information.

```bash
$ gbatch --version
<!-- cmdrun gbatch --version -->
```

## Script Directives

Embed job parameters in script using `# GFLOW` comments.

### Syntax

```bash
#!/bin/bash
# GFLOW --option value
# GFLOW --another-option value

# Your commands here
```

### Supported Directives

```bash
# GFLOW --gpus <N>
# GFLOW --time <TIME>
# GFLOW --priority <N>
# GFLOW --conda-env <ENV>
# GFLOW --depends-on <ID>
# GFLOW --depends-on-all <IDs>
# GFLOW --depends-on-any <IDs>
# GFLOW --no-auto-cancel
```

### Example Script

```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 2:00:00
# GFLOW --priority 20
# GFLOW --conda-env pytorch

echo "Starting training at $(date)"
python train.py --epochs 100
echo "Training complete at $(date)"
```

### Precedence

Command-line options override script directives:

```bash
# Script has: # GFLOW --time 1:00:00
# CLI overrides it:
gbatch --time 2:00:00 my_script.sh  # Uses 2 hours, not 1
```

## Template Creation

### `new` Subcommand

```bash
gbatch new <NAME>
```

Creates `<NAME>.sh` with template structure.

**Example**:
```bash
$ gbatch new my_job
Created template: my_job.sh

$ cat my_job.sh
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

**Usage**:
1. Create template: `gbatch new my_job`
2. Edit script: `vim my_job.sh`
3. Make executable: `chmod +x my_job.sh`
4. Submit: `gbatch my_job.sh`

## Environment Variables

gflow sets these variables in your job:

| Variable | Description | Example |
|----------|-------------|---------|
| `CUDA_VISIBLE_DEVICES` | Allocated GPU IDs | `0,1` |
| `GFLOW_ARRAY_TASK_ID` | Array task ID (0 if not array) | `5` |

**Usage in scripts**:
```bash
#!/bin/bash
echo "GPUs: $CUDA_VISIBLE_DEVICES"
echo "Task: $GFLOW_ARRAY_TASK_ID"
python train.py
```

## Output

### Submission Success

```bash
$ gbatch python train.py
Submitted batch job 42 (silent-pump-6338)
```

**Format**: `Submitted batch job <ID> (<NAME>)`

### Job Logs

Output captured to: `~/.local/share/gflow/logs/<JOBID>.log`

**View logs**:
```bash
# View completed job
cat ~/.local/share/gflow/logs/42.log

# Follow running job
tail -f ~/.local/share/gflow/logs/42.log
```

## Examples

### Basic Submission

```bash
# Direct command
gbatch echo 'Hello, gflow!'

# Script file
gbatch my_script.sh

# With options
gbatch --gpus 1 --time 2:00:00 python train.py
```

### Resource Allocation

```bash
# GPU job
gbatch --gpus 2 python multi_gpu_train.py

# CPU job with time limit
gbatch --time 30 python preprocess.py

# Conda environment (explicit)
gbatch --conda-env myenv python script.py

# Conda environment (auto-detected from currently active env)
conda activate myenv
gbatch python script.py  # Automatically uses 'myenv'
```

### Job Dependencies

```bash
# Sequential pipeline
gbatch --time 30 python prep.py

gbatch --depends-on @ --gpus 1 python train.py

gbatch --depends-on @ python eval.py
```

Using `@` references the most recently submitted job.

### Job Arrays

```bash
# Process 10 tasks in parallel
gbatch --array 1-10 python process.py --id $GFLOW_ARRAY_TASK_ID

# GPU sweep
gbatch --array 1-5 --gpus 1 --time 4:00:00 \
       python train.py --lr $(echo "0.001 0.01 0.1 0.5 1.0" | cut -d" " -f$GFLOW_ARRAY_TASK_ID)
```

### Priority and Scheduling

```bash
# Urgent job
gbatch --priority 100 --gpus 1 python urgent.py

# Background job
gbatch --priority 1 python background.py

# Named job
gbatch --name "exp-baseline" python train_baseline.py
```

### Script Templates

```bash
# Create template
gbatch new experiment

# Edit it
vim experiment.sh

# Submit it
gbatch experiment.sh
```

## Validation and Errors

### Common Errors

```bash
# Both script and command
Error: Cannot specify both script and direct command

# Script not found
Error: Script file not found: missing.sh

# Invalid dependency
Error: Dependency job 999 not found

# Circular dependency
Error: Circular dependency detected

# Invalid time format
Error: Invalid time format. Use HH:MM:SS, MM:SS, or MM

# Conda env not found
Error: Conda environment 'invalid_env' not found
```

### Validation Checks

At submission, gbatch validates:
- ✅ Script exists (if using script mode)
- ✅ Dependency job exists
- ✅ No circular dependencies
- ✅ Valid time format
- ✅ Not both script and direct command
- ✅ GPU count is reasonable

## Integration Examples

### Shell Script: Batch Submission

```bash
#!/bin/bash
# submit_experiments.sh - Submit multiple experiments

for lr in 0.001 0.01 0.1; do
    for bs in 32 64 128; do
        gbatch --gpus 1 --time 4:00:00 \
               --name "lr${lr}_bs${bs}" \
               python train.py --lr $lr --batch-size $bs
    done
done
```

### Shell Script: Pipeline Submission

```bash
#!/bin/bash
# submit_pipeline.sh - Submit data processing pipeline

set -e

echo "Submitting pipeline..."

# Stage 1: Download
gbatch --time 1:00:00 --name "download" python download.py
echo "Job download submitted"

# Stage 2: Process (depends on download)
gbatch --depends-on @ --time 2:00:00 --name "process" python process.py
echo "Job process submitted"

# Stage 3: Train (depends on process)
gbatch --depends-on @ --gpus 1 --time 8:00:00 --name "train" python train.py
echo "Job train submitted"

echo "Pipeline submitted! Monitor with: watch gqueue -t"
```

### Python Script: Job Submission

```python
#!/usr/bin/env python3
# submit_jobs.py - Submit jobs from Python

import subprocess
import re

def submit_job(command, **kwargs):
    """Submit a job and return its ID."""
    cmd = ['gbatch']

    # Add options first
    if 'gpus' in kwargs:
        cmd += ['--gpus', str(kwargs['gpus'])]
    if 'time' in kwargs:
        cmd += ['--time', kwargs['time']]
    if 'priority' in kwargs:
        cmd += ['--priority', str(kwargs['priority'])]
    if 'depends_on' in kwargs:
        cmd += ['--depends-on', str(kwargs['depends_on'])]
    if 'name' in kwargs:
        cmd += ['--name', kwargs['name']]

    # Add command at the end
    cmd += command.split() if isinstance(command, str) else command

    result = subprocess.run(cmd, capture_output=True, text=True)
    match = re.search(r'job (\d+)', result.stdout)
    return int(match.group(1)) if match else None

# Submit pipeline
prep_id = submit_job('python preprocess.py', time='30', name='prep')
train_id = submit_job('python train.py', gpus=1, time='4:00:00',
                       depends_on=prep_id, name='train')
eval_id = submit_job('python evaluate.py', time='10',
                      depends_on=train_id, name='eval')

print(f"Pipeline: {prep_id} -> {train_id} -> {eval_id}")
```

## Best Practices

1. **Always set time limits** for production jobs
   ```bash
   gbatch --time 2:00:00 your_command
   ```

2. **Use meaningful names** for easier tracking
   ```bash
   gbatch --name "exp-baseline-lr0.01" your_command
   ```

3. **Test scripts locally** before submitting
   ```bash
   bash my_job.sh  # Test first
   gbatch my_job.sh  # Then submit
   ```

4. **Request only needed GPUs**
   ```bash
   gbatch --gpus 1 "..."  # Not --gpus 4 if you only need 1
   ```

5. **Use dependencies** for workflows
   ```bash
   gbatch python prep.py
   gbatch --depends-on @ python train.py
   ```

6. **Use job arrays** for parallel tasks
   ```bash
   gbatch --array 1-10 python process.py --id $GFLOW_ARRAY_TASK_ID
   ```

7. **Add error handling** in scripts
   ```bash
   #!/bin/bash
   set -e  # Exit on error
   ```

8. **Log important info** in your jobs
   ```bash
   echo "Job started: $(date)"
   echo "GPUs: $CUDA_VISIBLE_DEVICES"
   ```

## See Also

- [gqueue](./gqueue-reference) - Job queue reference
- [gcancel](./gcancel-reference) - Job cancellation reference
- [ginfo](./ginfo-reference) - Scheduler inspection reference
- [Job Submission](../user-guide/job-submission) - Detailed submission guide
- [Job Dependencies](../user-guide/job-dependencies) - Workflow management
- [GPU Management](../user-guide/gpu-management) - GPU allocation guide
- [Time Limits](../user-guide/time-limits) - Time limit documentation
- [Quick Reference](./quick-reference) - Command cheat sheet
