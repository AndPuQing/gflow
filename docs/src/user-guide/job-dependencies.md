# Job Dependencies

This guide covers how to create complex workflows using job dependencies in gflow.

## Overview

Job dependencies allow you to create workflows where jobs wait for other jobs to complete before starting. This is essential for:
- Multi-stage pipelines (preprocessing → training → evaluation)
- Sequential workflows with data dependencies
- Conditional execution based on previous results
- Resource optimization (release GPUs between stages)

## Basic Usage

### Simple Dependency

Submit a job that depends on another:

```bash
# Job 1: Preprocessing
$ gbatch --name "prep" python preprocess.py
Submitted batch job 1 (prep)

# Job 2: Training (waits for job 1)
$ gbatch --depends-on 1 --name "train" python train.py
Submitted batch job 2 (train)
```

**How it works**:
- Job 2 starts only after Job 1 completes successfully (state: `Finished`)
- If Job 1 fails, Job 2 remains in `Queued` state indefinitely
- You must manually cancel Job 2 if Job 1 fails

**@ Syntax Sugar**: You can reference recent submissions without copying IDs:
- `--depends-on @` - Most recent submission (last job submitted)
- `--depends-on @~1` - Second most recent submission
- `--depends-on @~2` - Third most recent submission
- And so on...

This makes creating pipelines much simpler!

### Checking Dependencies

View dependency relationships:

```bash
$ gqueue -t
JOBID    NAME      ST    TIME         TIMELIMIT
1        prep      CD    00:02:15     UNLIMITED
└─ 2     train     R     00:05:30     04:00:00
   └─ 3  eval      PD    00:00:00     00:10:00
```

The tree view (`-t`) shows the dependency hierarchy with ASCII art.

## Creating Workflows

### Linear Pipeline

Execute jobs in sequence using @ syntax:

```bash
# Stage 1: Data collection
gbatch --time 10 python collect_data.py

# Stage 2: Data preprocessing (depends on stage 1)
gbatch --time 30 --depends-on @ python preprocess.py

# Stage 3: Training (depends on stage 2)
gbatch --time 4:00:00 --gpus 1 --depends-on @ python train.py

# Stage 4: Evaluation (depends on stage 3)
gbatch --time 10 --depends-on @ python evaluate.py
```

**Watch the pipeline**:
```bash
watch -n 5 gqueue -t
```

**How it works**: Each `--depends-on @` references the job submitted immediately before it, creating a clean sequential pipeline.

### Parallel Processing with Join

Multiple jobs feeding into one:

```bash
# Parallel data processing tasks
gbatch --time 30 python process_part1.py
gbatch --time 30 python process_part2.py
gbatch --time 30 python process_part3.py

# Merge results (waits for the last parallel task)
gbatch --depends-on @ python merge_results.py
```

**Current limitation**: gflow currently supports only one dependency per job. The example above shows `merge_results.py` depending on the last parallel task (`process_part3.py`). For true multi-parent dependencies (waiting for ALL parallel tasks), you need intermediate coordination jobs or submit merge after checking all parallel jobs completed.

### Branching Workflow

One job triggering multiple downstream jobs:

```bash
# Main processing
gbatch --time 1:00:00 python main_process.py

# Multiple analysis jobs (all depend on the main job)
gbatch --depends-on @ --time 30 python analysis_a.py
gbatch --depends-on @~1 --time 30 python analysis_b.py
gbatch --depends-on @~2 --time 30 python analysis_c.py
```

**Explanation**:
- First analysis depends on `@` (the main_process job)
- Second analysis depends on `@~1` (skipping analysis_a, back to main_process)
- Third analysis depends on `@~2` (skipping analysis_a and analysis_b, back to main_process)

## Dependency States and Behavior

### When Dependencies Start

A job with dependencies transitions from `Queued` to `Running` when:
1. The dependency job reaches `Finished` state
2. Required resources (GPUs, etc.) are available

### Failed Dependencies

If a dependency job fails:
- The dependent job remains in `Queued` state
- It will **never** start automatically
- You must manually cancel it with `gcancel`

**Example**:
```bash
# Job 1 fails
$ gqueue
JOBID    NAME      ST    TIME
1        prep      F     00:01:23
2        train     PD    00:00:00

# Job 2 will never run - must cancel it
$ gcancel 2
```

### Timeout Dependencies

If a dependency job times out:
- State changes to `Timeout` (TO)
- Treated the same as `Failed`
- Dependent jobs remain queued

### Cancelled Dependencies

If you cancel a job with dependencies:
- The job is cancelled
- Dependent jobs remain in queue (won't start)
- Use `gcancel --dry-run` to see impact before cancelling

**Check cancellation impact**:
```bash
$ gcancel --dry-run 1
Would cancel job 1 (prep)
Warning: The following jobs depend on job 1:
  - Job 2 (train)
  - Job 3 (eval)
These jobs will never start if job 1 is cancelled.
```

## Dependency Visualization

### Tree View

The tree view shows job dependencies clearly:

```bash
$ gqueue -t
JOBID    NAME           ST    TIME         TIMELIMIT
1        data-prep      CD    00:05:23     01:00:00
├─ 2     train-model-a  R     00:15:45     04:00:00
│  └─ 4  eval-a         PD    00:00:00     00:10:00
└─ 3     train-model-b  R     00:15:50     04:00:00
   └─ 5  eval-b         PD    00:00:00     00:10:00
```

**Legend**:
- `├─`: Branch connection
- `└─`: Last child connection
- `│`: Continuation line

### Circular Dependency Detection

gflow detects and prevents circular dependencies:

```bash
# This will fail
$ gbatch --depends-on 2 python a.py
Submitted batch job 1

$ gbatch --depends-on 1 python b.py
Error: Circular dependency detected: Job 2 depends on Job 1, which depends on Job 2
```

**Protection**:
- Validation happens at submission time
- Prevents deadlocks in the job queue
- Ensures all dependencies can eventually resolve

## Advanced Patterns

### Checkpointed Pipeline

Resume from failure points:

```bash
#!/bin/bash
# pipeline.sh - Resume from checkpoints

set -e

if [ ! -f "data.pkl" ]; then
    echo "Stage 1: Preprocessing"
    python preprocess.py
fi

if [ ! -f "model.pth" ]; then
    echo "Stage 2: Training"
    python train.py
fi

echo "Stage 3: Evaluation"
python evaluate.py
```

Submit:
```bash
gbatch --gpus 1 --time 8:00:00 pipeline.sh
```

### Conditional Dependency Script

Create a script that submits jobs based on previous results:

```bash
#!/bin/bash
# conditional_submit.sh

# Wait for job 1 to complete
while [ "$(gqueue -j 1 -f ST | tail -n 1)" = "R" ]; do
    sleep 5
done

# Check if it succeeded
STATUS=$(gqueue -j 1 -f ST | tail -n 1)

if [ "$STATUS" = "CD" ]; then
    echo "Job 1 succeeded, submitting next job"
    gbatch python next_step.py
else
    echo "Job 1 failed with status: $STATUS"
    exit 1
fi
```

### Array Jobs with Dependencies

Create job arrays that depend on a preprocessing job:

```bash
# Preprocessing
gbatch --time 30 python preprocess.py

# Array of training jobs (all depend on preprocessing)
for i in {1..5}; do
    gbatch --depends-on @ --gpus 1 --time 2:00:00 \
           python train.py --fold $i
done
```

**Note**: All array jobs use `--depends-on @` which references the preprocessing job since it's always the most recent non-array submission before the loop starts.

### Resource-Efficient Pipeline

Release GPUs between stages:

```bash
# Stage 1: CPU-only preprocessing
gbatch --time 30 python preprocess.py

# Stage 2: GPU training
gbatch --depends-on @ --gpus 2 --time 4:00:00 python train.py

# Stage 3: CPU-only evaluation
gbatch --depends-on @ --time 10 python evaluate.py
```

**Benefit**: GPUs are only allocated when needed, maximizing resource utilization.

## Monitoring Dependencies

### Check Dependency Status

```bash
# View specific job and its dependencies
gqueue -j 1,2,3 -f JOBID,NAME,ST,TIME

# View all jobs in tree format
gqueue -t

# Filter by state and view dependencies
gqueue -s Queued,Running -t
```

### Watch Pipeline Progress

```bash
# Real-time monitoring
watch -n 2 'gqueue -t'

# Show only active jobs
watch -n 2 'gqueue -s Running,Queued -t'
```

### Identify Blocked Jobs

Find jobs waiting on dependencies:

```bash
# Show queued jobs with dependency info
gqueue -s Queued -t

# Check why a job is queued
gqueue -j 5 -f JOBID,NAME,ST
gqueue -t | grep -A5 "^5"
```

## Dependency Validation

### Submission-time Validation

`gbatch` validates dependencies when you submit:

✅ **Valid submissions**:
- Dependency job exists
- No circular dependencies
- Dependency is not the job itself

❌ **Invalid submissions**:
- Dependency job doesn't exist: `Error: Dependency job 999 not found`
- Circular dependency: `Error: Circular dependency detected`
- Self-dependency: `Error: Job cannot depend on itself`

### Runtime Behavior

During execution:
- Scheduler checks dependencies every 5 seconds
- Jobs start when dependencies are `Finished` AND resources are available
- Failed/timeout dependencies never trigger dependent jobs

## Practical Examples

### Example 1: ML Training Pipeline

```bash
# Complete ML pipeline using @ syntax
gbatch --time 20 python prepare_dataset.py

gbatch --depends-on @ --gpus 1 --time 8:00:00 \
       python train.py --output model.pth

gbatch --depends-on @ --time 15 \
       python evaluate.py --model model.pth

gbatch --depends-on @ --time 5 python generate_report.py
```

### Example 2: Data Processing Pipeline

```bash
#!/bin/bash
# Submit a data processing pipeline

echo "Submitting data processing pipeline..."

# Download data
gbatch --time 1:00:00 --name "download" python download_data.py

# Validate data
gbatch --depends-on @ --time 30 --name "validate" python validate_data.py

# Transform data
gbatch --depends-on @ --time 45 --name "transform" python transform_data.py

# Upload results
gbatch --depends-on @ --time 30 --name "upload" python upload_results.py

echo "Pipeline submitted. Monitor with: watch gqueue -t"
```

### Example 3: Hyperparameter Sweep with Evaluation

```bash
# Train multiple models
for lr in 0.001 0.01 0.1; do
    gbatch --gpus 1 --time 2:00:00 \
           python train.py --lr $lr --output model_$lr.pth
done

# Wait for all models, then evaluate
# (Depends on the last model trained)
gbatch --depends-on @ --time 30 \
       python compare_models.py --models model_*.pth
```

**Note**: For true multi-dependency support (waiting for ALL models), you would need to either:
- Use a script that checks job status before submitting
- Submit the comparison job manually after all training completes

## Troubleshooting

### Issue: Dependent job not starting

**Possible causes**:
1. Dependency job hasn't finished:
   ```bash
   gqueue -t
   ```

2. Dependency job failed:
   ```bash
   gqueue -j <dep_id> -f JOBID,ST
   ```

3. No resources available (GPUs):
   ```bash
   ginfo
   gqueue -s Running -f NODES,NODELIST
   ```

### Issue: Want to cancel a job with dependencies

**Solution**: Use dry-run first to see impact:
```bash
# See what would happen
gcancel --dry-run <job_id>

# Cancel if acceptable
gcancel <job_id>

# Cancel dependent jobs too if needed
gcancel <job_id>
gcancel <dependent_job_id>
```

### Issue: Circular dependency error

**Solution**: Review your dependency chain:
```bash
# Check the job sequence
gqueue -j <job_ids> -t

# Restructure to eliminate cycles
```

### Issue: Lost track of dependencies

**Solution**: Use tree view:
```bash
# Show all job relationships
gqueue -a -t

# Focus on specific jobs
gqueue -j 1,2,3,4,5 -t
```

## Best Practices

1. **Plan workflows** before submitting jobs
2. **Use meaningful names** for jobs in pipelines (`--name` flag)
3. **Use @ syntax** for simpler dependency chains
4. **Set appropriate time limits** for each stage
5. **Monitor pipelines** with `watch gqueue -t`
6. **Handle failures** by checking dependency status
7. **Use dry-run** before cancelling jobs with dependents
8. **Document pipelines** in submission scripts
9. **Test small** before submitting long pipelines
10. **Check logs** when dependencies fail

## Multi-Job Dependencies

gflow supports advanced dependency patterns where a job can depend on multiple parent jobs with different logic modes.

### AND Logic (All Dependencies Must Succeed)

Use `--depends-on-all` when a job should wait for **all** parent jobs to finish successfully:

```bash
# Run three preprocessing jobs in parallel
gbatch --time 30 python preprocess_part1.py  # Job 101
gbatch --time 30 python preprocess_part2.py  # Job 102
gbatch --time 30 python preprocess_part3.py  # Job 103

# Training waits for ALL preprocessing jobs to complete
gbatch --depends-on-all 101,102,103 --gpus 2 --time 4:00:00 python train.py
```

**With @ syntax**:
```bash
gbatch python preprocess_part1.py  # Job 101
gbatch python preprocess_part2.py  # Job 102
gbatch python preprocess_part3.py  # Job 103

# Use @ syntax to reference recent jobs
gbatch --depends-on-all @,@~1,@~2 --gpus 2 python train.py
```

### OR Logic (Any Dependency Must Succeed)

Use `--depends-on-any` when a job should start as soon as **any one** parent job finishes successfully:

```bash
# Try multiple data sources in parallel
gbatch --time 10 python fetch_from_source_a.py  # Job 201
gbatch --time 10 python fetch_from_source_b.py  # Job 202
gbatch --time 10 python fetch_from_source_c.py  # Job 203

# Process data from whichever source succeeds first
gbatch --depends-on-any 201,202,203 python process_data.py
```

**Use case**: Fallback scenarios where multiple approaches are tried in parallel, and you want to proceed with the first successful result.

### Auto-Cancellation

By default, when a parent job fails, all dependent jobs are **automatically cancelled**:

```bash
gbatch python preprocess.py  # Job 301 - fails

# Job 302 will be auto-cancelled when 301 fails
gbatch --depends-on 301 python train.py  # Job 302
```

**Disable auto-cancellation** if you want dependent jobs to remain queued:

```bash
gbatch --depends-on 301 --no-auto-cancel python train.py
```

**When auto-cancellation happens**:
- Parent job fails (state: `Failed`)
- Parent job is cancelled (state: `Cancelled`)
- Parent job times out (state: `Timeout`)

**Cascade cancellation**: If job A depends on B, and B depends on C, when C fails, both B and A are cancelled automatically.

### Cascade Redo

When a job fails and you fix the issue, you can use `gjob redo` with the `--cascade` flag to automatically redo all dependent jobs that were cancelled due to the failure:

```bash
# Job 301 fails, causing jobs 302 and 303 to be auto-cancelled
gbatch python preprocess.py  # Job 301 - fails
gbatch --depends-on 301 python train.py  # Job 302 - auto-cancelled
gbatch --depends-on 302 python evaluate.py  # Job 303 - auto-cancelled

# After fixing the issue, redo the failed job and all its dependents
gjob redo 301 --cascade
```

**What happens**:
1. Job 301 is resubmitted as a new job (e.g., Job 304)
2. Job 302 is automatically resubmitted as Job 305, with its dependency updated to Job 304
3. Job 303 is automatically resubmitted as Job 306, with its dependency updated to Job 305
4. The entire dependency chain is preserved with new job IDs

**Cascade scope**:
- Only redoes jobs in `Cancelled` state with reason `DependencyFailed`
- Handles transitive dependencies (A→B→C→D)
- Automatically updates all dependency references to point to new job IDs
- Preserves all original job parameters (GPUs, time limits, conda env, etc.)

**Example with complex workflow**:
```bash
# Original workflow
gbatch python stage1.py  # Job 100 - fails
gbatch --depends-on 100 python stage2a.py  # Job 101 - cancelled
gbatch --depends-on 100 python stage2b.py  # Job 102 - cancelled
gbatch --depends-on-all 101,102 python stage3.py  # Job 103 - cancelled

# Redo with cascade
$ gjob redo 100 --cascade
Resubmitting job 100 with parameters:
  Script:       stage1.py
  ...
Submitted batch job 104 (stage1-1)

Cascading to 3 dependent job(s)...
  Job 101 → Job 105 (stage2a-1)
  Job 102 → Job 106 (stage2b-1)
  Job 103 → Job 107 (stage3-1)

Cascade complete.
```

**Benefits**:
- Saves time by not having to manually resubmit each dependent job
- Maintains the exact same workflow structure
- Automatically handles complex dependency updates
- Reduces errors from manual resubmission

**Without cascade** (manual approach):
```bash
gjob redo 100  # Job 104
gjob redo 101 --depends-on 104  # Job 105
gjob redo 102 --depends-on 104  # Job 106
gjob redo 103 --depends-on-all 105,106  # Job 107
```

### Circular Dependency Detection

gflow automatically detects and prevents circular dependencies at submission time:

```bash
gbatch python job_a.py  # Job 1
gbatch --depends-on 1 python job_b.py  # Job 2

# This will be rejected with an error
gbatch --depends-on 2 python job_c.py --depends-on-all 1,2  # Would create cycle
```

**Error message**:
```
Circular dependency detected: Job 3 depends on Job 2, which has a path back to Job 3
```

### Complex Workflow Example

Combining AND and OR logic for sophisticated workflows:

```bash
# Stage 1: Try multiple data collection methods
gbatch --time 30 python collect_method_a.py  # Job 1
gbatch --time 30 python collect_method_b.py  # Job 2

# Stage 2: Process whichever collection succeeds first
gbatch --depends-on-any 1,2 --time 1:00:00 python process_data.py  # Job 3

# Stage 3: Run multiple preprocessing tasks in parallel
gbatch --depends-on 3 --time 30 python preprocess_features.py  # Job 4
gbatch --depends-on 3 --time 30 python preprocess_labels.py    # Job 5

# Stage 4: Training waits for both preprocessing tasks
gbatch --depends-on-all 4,5 --gpus 2 --time 8:00:00 python train.py  # Job 6

# Stage 5: Evaluation depends on training
gbatch --depends-on 6 --time 30 python evaluate.py  # Job 7
```

**Visualize with tree view**:
```bash
$ gqueue -t
JOBID    NAME                ST    TIME         TIMELIMIT
1        collect_method_a    CD    00:15:30     00:30:00
2        collect_method_b    F     00:10:00     00:30:00
└─ 3     process_data        CD    00:45:00     01:00:00
   ├─ 4  preprocess_features CD    00:20:00     00:30:00
   └─ 5  preprocess_labels   CD    00:18:00     00:30:00
      └─ 6  train            R     02:30:00     08:00:00
         └─ 7  evaluate      PD    00:00:00     00:30:00
```

## Limitations

**Remaining limitations**:
- No dependency on specific job states (e.g., "start when job X fails")
- No job groups or batch dependencies beyond max_concurrent

**Removed limitations** (now supported):
- ~~Only one dependency per job~~ → Now supports multiple dependencies with `--depends-on-all` and `--depends-on-any`
- ~~No automatic cancellation~~ → Now auto-cancels by default (can be disabled with `--no-auto-cancel`)

**Workarounds**:
- For state-based dependencies, use conditional scripts that check job status
- Use external workflow managers for very complex DAGs if needed

## See Also

- [Job Submission](./job-submission) - Complete job submission guide
- [Time Limits](./time-limits) - Managing job timeouts
- [Quick Reference](../reference/quick-reference) - Command cheat sheet
- [Quick Start](../getting-started/quick-start) - Basic usage examples
