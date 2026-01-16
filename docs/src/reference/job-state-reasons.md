# Job State Reasons

## Overview

The Job State Reason system provides detailed information about why jobs are in their current states. This helps users understand what's happening with their jobs, whether they're waiting, running, or have completed.

## The `JobStateReason` Enum

Jobs can have an optional `reason` field that explains why they are in their current state. The reason is represented by the `JobStateReason` enum:

```rust
pub enum JobStateReason {
    /// Job is on hold by user request
    JobHeldUser,
    /// Job is waiting for dependencies to complete
    WaitingForDependency,
    /// Job is waiting for available resources (GPUs, memory, etc.)
    WaitingForResources,
    /// Job was cancelled by user request
    CancelledByUser,
    /// Job was cancelled because a dependency failed
    DependencyFailed(u32),
    /// Job was cancelled due to system error
    SystemError(String),
}
```

## Reason Categories

### Queued/Hold States

#### 1. JobHeldUser

**When it's shown:** When a job is in the `Hold` state (put on hold by the user).

**Display format:** `(JobHeldUser)`

**Example:**
```bash
$ gsubmit my_script.sh
Submitted job 123

$ ghold 123
Job 123 put on hold.

$ gqueue
JOBID  NAME         ST  TIME     NODES  NODELIST(REASON)
123    my-job       H   0:00:00  2      (JobHeldUser)
```

#### 2. WaitingForDependency

**When it's shown:** When a job is queued and waiting for its dependencies to complete.

**Display format:** `(Dependency)`

**Example:**
```bash
$ gsubmit train.sh
Submitted job 100

$ gsubmit --depend 100 evaluate.sh
Submitted job 101

$ gqueue
JOBID  NAME         ST  TIME     NODES  NODELIST(REASON)
100    train        R   0:01:23  4      0,1,2,3
101    evaluate     PD  0:00:00  2      (Dependency)
```

#### 3. WaitingForResources

**When it's shown:** When a job is queued and waiting for available resources (GPUs, memory, etc.).

**Display format:** `(Resources)`

**Example:**
```bash
# All GPUs are in use
$ gqueue
JOBID  NAME         ST  TIME     NODES  NODELIST(REASON)
100    job-1        R   0:05:23  4      0,1,2,3
101    job-2        R   0:03:45  4      4,5,6,7
102    job-3        PD  0:00:00  2      (Resources)
```

### Cancellation States

#### 4. CancelledByUser

**When it's set:** When a user manually cancels a job using `gcancel`.

**Display format:** `(Cancelled)` - The explicit "CancelledByUser" reason is not shown since it's obvious from context.

**Note:** The reason is still stored internally as `CancelledByUser` for tracking purposes, but the display simply shows `(Cancelled)` to avoid redundancy.

**Example:**
```bash
$ gcancel 123
Job 123 cancelled.

$ gqueue
JOBID  NAME         ST  TIME     NODES  NODELIST(REASON)
123    my-job       CA  0:00:05  2      (Cancelled)
```

#### 5. DependencyFailed

**When it's set:** When a job is automatically cancelled because one of its dependencies failed.

**Display format:** `(DependencyFailed:JOB_ID)` where `JOB_ID` is the ID of the failed dependency.

**Example:**
```bash
# Job 124 depends on job 123
$ gsubmit --depend 123 my_script.sh
Submitted job 124

# Job 123 fails
$ gfail 123

# Job 124 is automatically cancelled
$ gqueue
JOBID  NAME         ST  TIME     NODES  NODELIST(REASON)
123    job-123      F   0:01:23  1      -
124    job-124      CA  0:00:00  1      (DependencyFailed:123)
```

**How it works:**
- When a job fails, the scheduler automatically finds all queued jobs that depend on it
- Only jobs with `auto_cancel_on_dependency_failure: true` (the default) are cancelled
- The reason is set to `DependencyFailed(failed_job_id)` to indicate which dependency caused the cancellation
- This applies to both single dependencies (`--depend`) and multiple dependencies (`--depend-ids`)

#### 6. SystemError

**When it's set:** When a job is cancelled or fails due to a system-level error (e.g., resource allocation failure, execution error).

**Display format:** `(SystemError:MESSAGE)` where `MESSAGE` describes the error.

**Example:**
```bash
JOBID  NAME         ST  TIME     NODES  NODELIST(REASON)
125    my-job       CA  0:00:00  8      (SystemError:Insufficient GPUs)
```

## Viewing Reasons in gqueue

The `NODELIST(REASON)` column in `gqueue` displays different information depending on the job state:

| Job State | Display |
|-----------|---------|
| **Running** | GPU IDs assigned to the job (e.g., `0,1,2`) |
| **Queued** | `(Dependency)` if waiting for dependencies, `(Resources)` if waiting for resources |
| **Hold** | `(JobHeldUser)` |
| **Cancelled** | `(Cancelled)` for user cancellations, or the specific reason (e.g., `(DependencyFailed:123)`) for automatic cancellations |
| **Failed** | `-` (no reason displayed currently) |
| **Finished** | `-` |
| **Timeout** | `-` |

## Dependency Cancellation Behavior

### Auto-Cancel on Dependency Failure

By default, when a job fails, all jobs that depend on it are automatically cancelled. This behavior can be controlled with the `auto_cancel_on_dependency_failure` field:

```bash
# Default behavior - dependent jobs are auto-cancelled
$ gsubmit --depend 123 my_script.sh

# Disable auto-cancel (job will remain queued even if dependency fails)
$ gsubmit --depend 123 --no-auto-cancel my_script.sh
```

### Dependency Modes

The cancellation logic respects the dependency mode:

- **All mode (default):** Job is cancelled if ANY dependency fails
- **Any mode:** Job is cancelled only if ALL dependencies fail

```bash
# All mode - job 126 is cancelled if either 123 or 124 fails
$ gsubmit --depend-ids 123,124 --dependency-mode all my_script.sh

# Any mode - job 127 is cancelled only if both 123 and 124 fail
$ gsubmit --depend-ids 123,124 --dependency-mode any my_script.sh
```

## Implementation Details

### Data Structure

The `reason` field is stored in the `Job` struct:

```rust
pub struct Job {
    // ... other fields ...

    #[serde(default)]
    pub reason: Option<JobStateReason>,
}
```

The `#[serde(default)]` attribute ensures backward compatibility with existing state files that don't have the `reason` field.

### When Reasons Are Set

1. **User Cancellation:** Set in `Scheduler::cancel_job()` when `reason` parameter is `None`
2. **Dependency Cancellation:** Set in `Scheduler::auto_cancel_dependent_jobs()` when a dependency fails
3. **System Errors:** Can be set by the executor or runtime when system-level errors occur

### Backward Compatibility

The reason system is fully backward compatible:
- Old state files without the `reason` field will deserialize correctly (reason will be `None`)
- Jobs without a reason will display `(Cancelled)` in gqueue instead of a specific reason
- The system continues to work normally even if reasons are not set

## Future Enhancements

Potential future additions to the `JobStateReason` enum:

- **ExecutionFailed(String):** For jobs that fail during execution with error details
- **TimeoutExceeded:** For jobs that exceed their time limit
- **MemoryExceeded:** For jobs that exceed their memory limit
- **ResourceUnavailable:** For jobs cancelled due to resource constraints
- **PreemptedByHigherPriority:** For jobs preempted by higher priority jobs

## Example Workflow

Here's a complete example showing how reasons work in practice:

```bash
# Submit a pipeline of jobs
$ gsubmit train_model.sh
Submitted job 100

$ gsubmit --depend 100 evaluate_model.sh
Submitted job 101

$ gsubmit --depend 101 deploy_model.sh
Submitted job 102

# Check status
$ gqueue
JOBID  NAME              ST  TIME     NODES  NODELIST(REASON)
100    train_model       R   0:05:23  4      0,1,2,3
101    evaluate_model    PD  0:00:00  2      (Dependency)
102    deploy_model      PD  0:00:00  1      (Dependency)

# Job 100 fails
$ gfail 100

# Check status again - dependent jobs are auto-cancelled
$ gqueue
JOBID  NAME              ST  TIME     NODES  NODELIST(REASON)
100    train_model       F   0:05:45  4      -
101    evaluate_model    CA  0:00:00  2      (DependencyFailed:100)
102    deploy_model      CA  0:00:00  1      (DependencyFailed:101)
```

Notice that:
- Job 101 was cancelled because its dependency (job 100) failed
- Job 102 was cancelled because its dependency (job 101) was cancelled
- The reason clearly indicates which job caused the cancellation

## See Also

- [Job Dependencies](./dependencies.md)
- [Job States](./job-states.md)
- [gqueue Command Reference](./gqueue.md)
