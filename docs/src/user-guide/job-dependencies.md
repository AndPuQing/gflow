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
$ gbatch --command "python preprocess.py" --name "prep"
Submitted batch job 1 (prep)

# Job 2: Training (waits for job 1)
$ gbatch --command "python train.py" --depends-on 1 --name "train"
Submitted batch job 2 (train)
```

**How it works**:
- Job 2 starts only after Job 1 completes successfully (state: `Finished`)
- If Job 1 fails, Job 2 remains in `Queued` state indefinitely
- You must manually cancel Job 2 if Job 1 fails

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

Execute jobs in sequence:

```bash
# Stage 1: Data collection
ID1=$(gbatch --time 10 --command "python collect_data.py" | grep -oP '\d+')

# Stage 2: Data preprocessing (depends on stage 1)
ID2=$(gbatch --time 30 --depends-on $ID1 --command "python preprocess.py" | grep -oP '\d+')

# Stage 3: Training (depends on stage 2)
ID3=$(gbatch --time 4:00:00 --gpus 1 --depends-on $ID2 --command "python train.py" | grep -oP '\d+')

# Stage 4: Evaluation (depends on stage 3)
gbatch --time 10 --depends-on $ID3 --command "python evaluate.py"
```

**Watch the pipeline**:
```bash
watch -n 5 gqueue -t
```

### Parallel Processing with Join

Multiple jobs feeding into one:

```bash
# Parallel data processing tasks
ID1=$(gbatch --time 30 --command "python process_part1.py" | grep -oP '\d+')
ID2=$(gbatch --time 30 --command "python process_part2.py" | grep -oP '\d+')
ID3=$(gbatch --time 30 --command "python process_part3.py" | grep -oP '\d+')

# Merge results (waits for all three)
# Note: Currently gflow supports single dependency per job
# For multiple dependencies, you'll need to chain them
gbatch --depends-on $ID3 --command "python merge_results.py"
```

**Current limitation**: gflow currently supports only one dependency per job. For multiple dependencies, create intermediate coordination jobs.

### Branching Workflow

One job triggering multiple downstream jobs:

```bash
# Main processing
ID1=$(gbatch --time 1:00:00 --command "python main_process.py" | grep -oP '\d+')

# Multiple analysis jobs (all depend on ID1)
gbatch --depends-on $ID1 --time 30 --command "python analysis_a.py"
gbatch --depends-on $ID1 --time 30 --command "python analysis_b.py"
gbatch --depends-on $ID1 --time 30 --command "python analysis_c.py"
```

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
$ gbatch --depends-on 2 --command "python a.py"
Submitted batch job 1

$ gbatch --depends-on 1 --command "python b.py"
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
    gbatch --command "python next_step.py"
else
    echo "Job 1 failed with status: $STATUS"
    exit 1
fi
```

### Array Jobs with Dependencies

Create job arrays that depend on a preprocessing job:

```bash
# Preprocessing
ID=$(gbatch --time 30 --command "python preprocess.py" | grep -oP '\d+')

# Array of training jobs (all depend on preprocessing)
for i in {1..5}; do
    gbatch --depends-on $ID --gpus 1 --time 2:00:00 \
           --command "python train.py --fold $i"
done
```

### Resource-Efficient Pipeline

Release GPUs between stages:

```bash
# Stage 1: CPU-only preprocessing
ID1=$(gbatch --time 30 --command "python preprocess.py" | grep -oP '\d+')

# Stage 2: GPU training
ID2=$(gbatch --depends-on $ID1 --gpus 2 --time 4:00:00 \
             --command "python train.py" | grep -oP '\d+')

# Stage 3: CPU-only evaluation
gbatch --depends-on $ID2 --time 10 --command "python evaluate.py"
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
# Complete ML pipeline
PREP=$(gbatch --time 20 --command "python prepare_dataset.py" | grep -oP '\d+')

TRAIN=$(gbatch --depends-on $PREP --gpus 1 --time 8:00:00 \
               --command "python train.py --output model.pth" | grep -oP '\d+')

EVAL=$(gbatch --depends-on $TRAIN --time 15 \
              --command "python evaluate.py --model model.pth" | grep -oP '\d+')

gbatch --depends-on $EVAL --time 5 \
       --command "python generate_report.py"
```

### Example 2: Data Processing Pipeline

```bash
#!/bin/bash
# Submit a data processing pipeline

echo "Submitting data processing pipeline..."

# Download data
ID1=$(gbatch --time 1:00:00 --name "download" \
             --command "python download_data.py" | grep -oP '\d+')

# Validate data
ID2=$(gbatch --depends-on $ID1 --time 30 --name "validate" \
             --command "python validate_data.py" | grep -oP '\d+')

# Transform data
ID3=$(gbatch --depends-on $ID2 --time 45 --name "transform" \
             --command "python transform_data.py" | grep -oP '\d+')

# Upload results
gbatch --depends-on $ID3 --time 30 --name "upload" \
       --command "python upload_results.py"

echo "Pipeline submitted. Monitor with: watch gqueue -t"
```

### Example 3: Hyperparameter Sweep with Evaluation

```bash
# Train multiple models
MODELS=()
for lr in 0.001 0.01 0.1; do
    ID=$(gbatch --gpus 1 --time 2:00:00 \
                --command "python train.py --lr $lr --output model_$lr.pth" | grep -oP '\d+')
    MODELS+=($ID)
done

# Wait for all models, then evaluate
# (Create a dummy job that depends on the last model)
LAST_MODEL=${MODELS[-1]}
gbatch --depends-on $LAST_MODEL --time 30 \
       --command "python compare_models.py --models model_*.pth"
```

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
   gctl info
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
2. **Use meaningful names** for jobs in pipelines
3. **Extract job IDs** for reliable dependency tracking
4. **Set appropriate time limits** for each stage
5. **Monitor pipelines** with `watch gqueue -t`
6. **Handle failures** by checking dependency status
7. **Use dry-run** before cancelling jobs with dependents
8. **Document pipelines** in submission scripts
9. **Test small** before submitting long pipelines
10. **Check logs** when dependencies fail

## Limitations

**Current limitations**:
- Only one dependency per job (no multi-parent dependencies)
- No automatic cancellation of dependents when parent fails
- No dependency on specific job states (e.g., "start when job X fails")
- No job groups or batch dependencies

**Workarounds**:
- For multiple dependencies, use intermediate coordination jobs
- Monitor job status and submit conditionally with scripts
- Use external workflow managers for complex DAGs if needed

## See Also

- [Job Submission](./job-submission.md) - Complete job submission guide
- [Time Limits](./time-limits.md) - Managing job timeouts
- [Quick Reference](../reference/quick-reference.md) - Command cheat sheet
- [Quick Start](../getting-started/quick-start.md) - Basic usage examples
