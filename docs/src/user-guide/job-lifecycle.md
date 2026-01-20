# Job Lifecycle

This guide explains the complete lifecycle of jobs in gflow, including state transitions, status checking, and recovery operations.

## Job States

gflow jobs can be in one of seven states:

| State | Short | Description |
|-------|-------|-------------|
| **Queued** | PD | Job is waiting to run (pending dependencies or resources) |
| **Hold** | H | Job is on hold by user request |
| **Running** | R | Job is currently executing |
| **Finished** | CD | Job completed successfully |
| **Failed** | F | Job terminated with an error |
| **Cancelled** | CA | Job was cancelled by user or system |
| **Timeout** | TO | Job exceeded its time limit |

### State Categories

**Active States** (job is not yet complete):
- Queued, Hold, Running

**Completed States** (job has finished):
- Finished, Failed, Cancelled, Timeout

## State Transition Diagram

The following diagram shows all possible state transitions in gflow:

```mermaid
stateDiagram-v2
    [*] --> Queued: Submit job

    Queued --> Running: Resources available &<br/>dependencies met
    Queued --> Hold: User holds job
    Queued --> Cancelled: User cancels or<br/>dependency fails

    Hold --> Queued: User releases job
    Hold --> Cancelled: User cancels job

    Running --> Finished: Job completes<br/>successfully
    Running --> Failed: Job exits with<br/>error code
    Running --> Cancelled: User cancels job
    Running --> Timeout: Time limit exceeded

    Finished --> [*]
    Failed --> [*]
    Cancelled --> [*]
    Timeout --> [*]
```

### State Transition Rules

**From Queued**:
- → **Running**: When dependencies are met AND resources are available
- → **Hold**: User runs `gjob hold <id>`
- → **Cancelled**: User runs `gcancel <id>` OR a dependency fails (with auto-cancel enabled)

**From Hold**:
- → **Queued**: User runs `gjob release <id>`
- → **Cancelled**: User runs `gcancel <id>`

**From Running**:
- → **Finished**: Job script/command exits with code 0
- → **Failed**: Job script/command exits with non-zero code
- → **Cancelled**: User runs `gcancel <id>`
- → **Timeout**: Job exceeds its time limit (set with `--time`)

**From Completed States**:
- No transitions (final states)
- Use `gjob redo <id>` to create a new job with the same parameters

## Job State Reasons

Jobs in certain states have an associated reason that provides more context:

| State | Reason | Description |
|-------|--------|-------------|
| Queued | `WaitingForDependency` | Job is waiting for parent jobs to finish |
| Queued | `WaitingForResources` | Job is waiting for available GPUs/memory |
| Hold | `JobHeldUser` | Job was put on hold by user request |
| Cancelled | `CancelledByUser` | User explicitly cancelled the job |
| Cancelled | `DependencyFailed:<id>` | Job was auto-cancelled because job `<id>` failed |
| Cancelled | `SystemError:<msg>` | Job was cancelled due to a system error |

View the reason with `gjob show <id>` or `gqueue -f JOBID,ST,REASON`.

## Status Checking Workflow

The following diagram shows how to check job status and take appropriate actions:

```mermaid
flowchart TD
    Start([Check Job Status]) --> CheckState{What is the<br/>job state?}

    CheckState -->|Queued| CheckReason{Check reason}
    CheckReason -->|WaitingForDependency| CheckDep[Check dependency status<br/>gqueue -t]
    CheckReason -->|WaitingForResources| CheckRes[Check resource availability<br/>ginfo]
    CheckDep --> DepOK{Dependency<br/>finished?}
    DepOK -->|Yes| Wait1[Wait for resources]
    DepOK -->|No| Wait2[Wait for dependency]
    CheckRes --> Wait1

    CheckState -->|Hold| Release[Release job<br/>gjob release ID]
    Release --> Queued2[Job moves to Queued]

    CheckState -->|Running| Monitor[Monitor progress<br/>gjob log ID<br/>gjob attach ID]
    Monitor --> Wait3[Wait for completion]

    CheckState -->|Finished| Success([Job completed<br/>successfully])

    CheckState -->|Failed| Investigate[Check logs<br/>gjob log ID]
    Investigate --> Fix{Can fix?}
    Fix -->|Yes| Redo[Redo job<br/>gjob redo ID]
    Fix -->|No| End1([Investigation needed])

    CheckState -->|Cancelled| CheckCancelReason{Check<br/>reason}
    CheckCancelReason -->|CancelledByUser| End2([User action])
    CheckCancelReason -->|DependencyFailed| RedoCascade[Fix parent and redo<br/>gjob redo PARENT_ID --cascade]

    CheckState -->|Timeout| IncreaseTime[Increase time limit<br/>gjob redo ID --time HH:MM:SS]

    Wait1 --> Start
    Wait2 --> Start
    Wait3 --> Start
    Redo --> Start
    RedoCascade --> Start
    IncreaseTime --> Start
```

## See Also

- [Job Dependencies](./job-dependencies) - Complete guide to job dependencies
- [Job Submission](./job-submission) - Job submission options
- [Time Limits](./time-limits) - Managing job timeouts
<!--- [gjob Reference](../reference/gjob-reference) - Complete command reference-->
