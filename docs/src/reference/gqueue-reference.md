# gqueue Command Reference

Complete reference for the `gqueue` command - gflow's job queue monitoring tool.

## Synopsis

```bash
gqueue [OPTIONS]
```

## Description

`gqueue` displays information about jobs in the gflow queue. It provides flexible filtering, formatting, and visualization options similar to Slurm's `squeue` command.

## Options

### Filtering Options

#### `--states <STATES>`, `-s <STATES>`

Filter jobs by state (comma-separated list).

**Valid states**:
- `Queued` or `PD`: Waiting for resources or dependencies
- `Running` or `R`: Currently executing
- `Finished` or `CD`: Completed successfully
- `Failed` or `F`: Exited with error
- `Cancelled` or `CA`: Manually cancelled
- `Timeout` or `TO`: Exceeded time limit

**Examples**:
```bash
# Show only running jobs
gqueue -s Running

# Show running and queued jobs
gqueue -s Running,Queued

# Show all terminal states
gqueue -s Finished,Failed,Cancelled,Timeout

# Use short form
gqueue -s R,PD
```

#### `--jobs <IDS>`, `-j <IDS>`

Filter by job ID (comma-separated list or ranges).

**Examples**:
```bash
# Single job
gqueue -j 42

# Multiple jobs
gqueue -j 1,2,3

# Range (if supported)
gqueue -j 1-10

# Mixed
gqueue -j 1,5,10-15
```

#### `--names <NAMES>`, `-N <NAMES>`

Filter by job name (comma-separated list).

**Examples**:
```bash
# Single name
gqueue -N training-job

# Multiple names
gqueue -N "prep,train,eval"

# Pattern matching (depends on implementation)
gqueue -N "train*"
```

### Display Options

#### `--limit <NUM>`, `-n <NUM>`

Limit number of jobs displayed.

**Behavior**:
- Positive number: Show first N jobs
- Negative number: Show last N jobs (default: -10)
- Zero: Show all jobs (same as `--all`)

**Examples**:
```bash
# Show last 10 jobs (default)
gqueue

# Show last 20 jobs
gqueue -n 20

# Show first 5 jobs
gqueue -n 5

# Show all jobs
gqueue -n 0
```

#### `--all`, `-a`

Show all jobs (equivalent to `-n 0`).

**Example**:
```bash
gqueue -a
```

#### `--format <FIELDS>`, `-f <FIELDS>`

Custom output format (comma-separated field list).

**Available fields**:
- `JOBID`: Job ID number
- `NAME`: Job name (tmux session name)
- `ST`: Job state (short form: PD, R, CD, F, CA, TO)
- `STATE`: Job state (long form: Queued, Running, Finished, etc.)
- `TIME`: Elapsed time (HH:MM:SS)
- `TIMELIMIT`: Time limit (HH:MM:SS or UNLIMITED)
- `NODES`: Number of GPUs requested
- `NODELIST`: GPU IDs assigned (or REASON for queued jobs)
- `NODELIST(REASON)`: Alias for NODELIST
- `PRIORITY`: Job priority (0-255)
- `DEPENDENCY`: Job ID this job depends on
- `USER`: Username of the job submitter

**Default format**:
```bash
JOBID,NAME,ST,TIME,TIMELIMIT,NODES,NODELIST(REASON)
```

**Examples**:
```bash
# Minimal output
gqueue -f JOBID,NAME,ST

# Time-focused view
gqueue -f JOBID,TIME,TIMELIMIT

# Resource-focused view
gqueue -f JOBID,NODES,NODELIST,STATE

# Full info
gqueue -f JOBID,NAME,STATE,TIME,TIMELIMIT,NODES,NODELIST,PRIORITY,DEPENDENCY
```

#### `--group`, `-g`

Group jobs by state.

**Example**:
```bash
$ gqueue -g
RUNNING:
JOBID    NAME                ST    TIME         TIMELIMIT
1        train-resnet        R     00:15:23     04:00:00
2        train-vit           R     00:12:45     04:00:00

QUEUED:
JOBID    NAME                ST    TIME         TIMELIMIT
3        eval-models         PD    00:00:00     00:30:00

FINISHED:
JOBID    NAME                ST    TIME         TIMELIMIT
4        preprocess          CD    00:05:12     01:00:00
```

#### `--tree`, `-t`

Display jobs in dependency tree format.

**Example**:
```bash
$ gqueue -t
JOBID    NAME           ST    TIME         TIMELIMIT
1        data-prep      CD    00:05:23     01:00:00
├─ 2     train-model-a  R     00:15:45     04:00:00
│  └─ 4  eval-a         PD    00:00:00     00:10:00
└─ 3     train-model-b  R     00:15:50     04:00:00
   └─ 5  eval-b         PD    00:00:00     00:10:00
```

**Features**:
- Shows parent-child relationships
- Visualizes workflow structure
- Detects and handles circular dependencies gracefully
- ASCII tree drawing with box-drawing characters

### Sorting Options

#### `--sort <FIELD>`, `-r <FIELD>`

Sort jobs by field.

**Valid fields**:
- `id`: Job ID (default)
- `state`: Job state
- `time`: Start time
- `name`: Job name
- `gpus`: Number of GPUs
- `priority`: Job priority

**Examples**:
```bash
# Sort by priority (high to low)
gqueue -r priority

# Sort by name
gqueue -r name

# Sort by elapsed time
gqueue -r time

# Sort by GPU count
gqueue -r gpus
```

**Note**: Sorting works with filtering and formatting options.

### Global Options

#### `--config <PATH>`

Use custom configuration file (hidden option).

**Example**:
```bash
gqueue --config /path/to/custom.toml
```

#### `--help`, `-h`

Display help message.

```bash
$ gqueue --help
<!-- cmdrun gqueue --help -->
```

#### `--version`, `-V`

Display version information.

```bash
$ gqueue --version
<!-- cmdrun gqueue --version -->
```

## Output Format

### Default Output

```
JOBID    NAME                ST    TIME         TIMELIMIT    NODES    NODELIST(REASON)
1        silent-pump-6338    R     00:15:23     02:00:00     1        0
2        brave-river-1234    PD    00:00:00     04:00:00     2        (Resources)
3        gentle-wave-9876    CD    00:45:12     UNLIMITED    0        N/A
```

### Column Descriptions

| Column | Description | Example |
|--------|-------------|---------|
| JOBID | Unique job identifier | 42 |
| NAME | Job run name (tmux session) | silent-pump-6338 |
| ST | State (short) | R, PD, CD, F, CA, TO |
| STATE | State (long) | Running, Queued, Finished |
| TIME | Elapsed time | 00:15:23 |
| TIMELIMIT | Maximum runtime | 02:00:00, UNLIMITED |
| NODES | GPU count | 0, 1, 2 |
| NODELIST(REASON) | GPU IDs or wait reason | 0,1 or (Resources) |
| PRIORITY | Job priority | 10 (default) |
| DEPENDENCY | Parent job ID | 5 (or N/A) |

### State Codes

| Code | Full State | Meaning |
|------|------------|---------|
| PD | Queued | Waiting for resources or dependencies |
| R | Running | Currently executing |
| CD | Finished | Completed successfully |
| F | Failed | Exited with non-zero status |
| CA | Cancelled | Manually cancelled |
| TO | Timeout | Exceeded time limit |

### Time Format

**Elapsed time and time limits**:
- Format: `HH:MM:SS` or `D-HH:MM:SS` (with days)
- Examples:
  - `00:15:23`: 15 minutes, 23 seconds
  - `02:30:00`: 2 hours, 30 minutes
  - `1-04:30:00`: 1 day, 4 hours, 30 minutes
  - `UNLIMITED`: No time limit

### Node List Format

**For running jobs**: Comma-separated GPU IDs
```
0,1,2
```

**For queued jobs**: Reason for waiting
```
(Resources)
(Dependency: Job 5)
```

**For non-GPU jobs**: N/A
```
N/A
```

## Examples

### Basic Usage

```bash
# View last 10 jobs
gqueue

# View all jobs
gqueue -a

# View last 20 jobs
gqueue -n 20
```

### Filtering

```bash
# Show only running jobs
gqueue -s Running

# Show running and queued jobs
gqueue -s Running,Queued

# Show specific job
gqueue -j 42

# Show multiple jobs
gqueue -j 40,41,42

# Show jobs by name
gqueue -N "training-job"
```

### Custom Formatting

```bash
# Minimal view
gqueue -f JOBID,NAME,ST

# Time-focused
gqueue -f JOBID,NAME,TIME,TIMELIMIT

# GPU-focused
gqueue -f JOBID,NAME,NODES,NODELIST

# Priority queue view
gqueue -f JOBID,NAME,PRIORITY,ST -r priority
```

### Visualization

```bash
# Group by state
gqueue -g

# Show dependency tree
gqueue -t

# Tree view with filtering
gqueue -s Running,Queued -t
```

### Sorting

```bash
# Sort by priority (highest first)
gqueue -r priority

# Sort by elapsed time
gqueue -r time

# Sort by job ID (default)
gqueue -r id
```

### Monitoring

```bash
# Watch queue in real-time
watch -n 2 gqueue

# Watch running jobs
watch -n 2 'gqueue -s Running'

# Watch with custom format
watch -n 2 'gqueue -f JOBID,NAME,TIME,TIMELIMIT'

# Monitor dependency tree
watch -n 2 'gqueue -t'
```

### Combined Options

```bash
# Running GPU jobs with details
gqueue -s Running -f JOBID,NAME,NODES,NODELIST,TIME

# Last 5 finished jobs
gqueue -s Finished -n 5 -r time

# All queued jobs grouped
gqueue -s Queued -g

# High-priority jobs first
gqueue -r priority -n 20
```

## Common Patterns

### Check Job Status

```bash
# Is job 42 running?
gqueue -j 42 -f ST

# What's the status of my job?
gqueue -N "my-job-name" -f JOBID,ST
```

### Monitor Pipeline

```bash
# View workflow
gqueue -t

# Watch pipeline progress
watch -n 2 'gqueue -t'
```

### Find Stuck Jobs

```bash
# Jobs queued for long time
gqueue -s Queued -r time -f JOBID,NAME,TIME

# Why is my job queued?
gqueue -j 42 -t
```

### Resource Monitoring

```bash
# What GPUs are in use?
gqueue -s Running -f JOBID,NAME,NODES,NODELIST

# How many jobs are waiting for GPUs?
gqueue -s Queued -f JOBID,NODELIST
```

### Job History

```bash
# Recent completions
gqueue -s Finished -n 10 -r time

# Failed jobs
gqueue -s Failed -a

# Timed out jobs
gqueue -s Timeout -a
```

## Integration Examples

### Shell Scripts

```bash
#!/bin/bash
# Wait for job to complete

JOB_ID=42

while true; do
    STATUS=$(gqueue -j $JOB_ID -f ST | tail -n 1)

    if [ "$STATUS" = "CD" ]; then
        echo "Job completed successfully!"
        break
    elif [ "$STATUS" = "F" ] || [ "$STATUS" = "TO" ]; then
        echo "Job failed or timed out!"
        exit 1
    fi

    sleep 5
done
```

### Pipeline Monitoring

```bash
#!/bin/bash
# Monitor pipeline progress

echo "=== Pipeline Status ==="
gqueue -j 1,2,3,4,5 -t

echo -e "\n=== Running Jobs ==="
gqueue -s Running -f JOBID,NAME,TIME,TIMELIMIT

echo -e "\n=== Queued Jobs ==="
gqueue -s Queued -f JOBID,NAME,NODELIST
```

### Resource Dashboard

```bash
#!/bin/bash
# Simple resource dashboard

clear
echo "╔════════════════════════════════════════╗"
echo "║         gflow Resource Dashboard       ║"
echo "╚════════════════════════════════════════╝"

echo -e "\n📊 Running Jobs:"
gqueue -s Running -f JOBID,NAME,NODES,NODELIST

echo -e "\n⏳ Queued Jobs:"
gqueue -s Queued -f JOBID,NAME,NODES,NODELIST

echo -e "\n✅ Recently Completed:"
gqueue -s Finished -n 5 -r time -f JOBID,NAME,TIME
```

### Job Stats

```bash
#!/bin/bash
# Job statistics

TOTAL=$(gqueue -a -f JOBID | tail -n +2 | wc -l)
RUNNING=$(gqueue -s Running -f JOBID | tail -n +2 | wc -l)
QUEUED=$(gqueue -s Queued -f JOBID | tail -n +2 | wc -l)
FINISHED=$(gqueue -s Finished -f JOBID | tail -n +2 | wc -l)
FAILED=$(gqueue -s Failed -f JOBID | tail -n +2 | wc -l)

echo "Total jobs: $TOTAL"
echo "Running: $RUNNING"
echo "Queued: $QUEUED"
echo "Finished: $FINISHED"
echo "Failed: $FAILED"
```

## Troubleshooting

### Empty Output

**Possible causes**:
1. No jobs in queue
2. All jobs filtered out by state/name/id filter
3. Daemon not running

**Solutions**:
```bash
# Check daemon
ginfo

# View all jobs
gqueue -a

# Remove filters
gqueue
```

### Formatting Issues

**Issue**: Columns misaligned

**Solution**: Terminal too narrow or too many columns
```bash
# Use fewer columns
gqueue -f JOBID,NAME,ST

# Increase terminal width
```

### State Not Updating

**Issue**: Job state seems stale

**Solution**: Daemon updates state every 5 seconds
```bash
# Wait a few seconds
sleep 5
gqueue

gflowd down
gflowd up
```

## Performance Notes

- `gqueue` is fast even with thousands of jobs
- Filtering by ID is faster than by name
- Tree view may be slow with very deep dependencies
- Default limit (-10) keeps output manageable

## See Also

- [gbatch](./gbatch-reference) - Job submission reference
- [gcancel](./gcancel-reference) - Job cancellation reference
- [ginfo](./ginfo-reference) - Scheduler inspection reference
- [Quick Reference](./quick-reference) - Command cheat sheet
- [Job Submission](../user-guide/job-submission) - Job submission guide
- [Job Dependencies](../user-guide/job-dependencies) - Dependency management
