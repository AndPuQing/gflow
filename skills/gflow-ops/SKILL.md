---
name: gflow-ops
description: Use when working with the local gflow scheduler: checking daemon health, inspecting jobs or GPU state, reading logs, submitting test jobs, or updating, holding, releasing, or cancelling jobs. This skill is for local-first gflow operations through MCP tools or the gflow CLI.
---

# gflow Ops

Use this skill for local `gflow` work: inspect the daemon, queue, jobs, reservations, logs, and safely mutate scheduler state.

## When To Use

- The user asks about `gflowd`, `gqueue`, `gjob`, `gcancel`, `ginfo`, `gstats`, or the local scheduler state.
- The task needs job inspection, failure diagnosis, or GPU availability checks.
- The task needs job submission, update, hold, release, or cancellation.
- The task needs validation of the local `gflow mcp serve` MCP server.

Do not use this skill for remote cluster orchestration or multi-tenant service design. `gflow` is local-first.

## Preferred Order

1. Check liveness first.
   Use MCP `get_health` if available. CLI fallback: `gflowd status`.
2. Inspect before mutating.
   Use `get_info`, `list_jobs`, `get_job`, `get_stats`, `list_reservations`.
3. Prefer MCP tools over ad hoc shell commands when the tool server is available.
4. For validation, use short no-GPU test jobs in the current repo directory.
5. Clean up test jobs when done.

## MCP Tools

Prefer these tools when the local MCP server is configured:

- `get_health`
- `get_info`
- `list_jobs`
- `get_job`
- `get_job_log`
- `get_stats`
- `list_reservations`
- `submit_job`
- `submit_jobs_batch`
- `update_job`
- `hold_job`
- `release_job`
- `cancel_job`

`get_job_log.text` is the agent-friendly program output view.
Use `full_text` only when the filtered output is missing important context.
Use `submit_job` for one-off test jobs and `submit_jobs_batch` when creating multiple related jobs in one mutation.

## CLI Fallbacks

Use CLI only when MCP is unavailable or when testing the CLI itself.

- Health: `gflowd status`
- Queue: `gqueue`, `gqueue -a`, `gqueue -s Running`
- Scheduler info: `ginfo`, `gstats`
- Submit: `gbatch ...`
- Inspect: `gjob show <job_id>`, `gjob log <job_id>`
- Mutate: `gjob hold <job_id>`, `gjob release <job_id>`, `gjob update ...`, `gcancel <job_id>`

## Safety Rules

- Treat all write operations as potentially user-impacting.
- Do not cancel, hold, release, or update existing non-test jobs without explicit user intent.
- For experimentation, create dedicated test jobs with `gpus: 0` unless the user explicitly wants GPU scheduling behavior.
- Before using `cancel_job`, confirm the target job ID and whether it is a test job.
- After creating test jobs, cancel them if they are still active.

## Reliable Validation Pattern

When you need a queued job for `hold_job`, `release_job`, or `update_job` testing:

1. Submit a blocker job with `gpus: 0` and a long `sleep`.
2. Submit a dependent job with `depends_on_ids: [blocker_id]`.
3. Operate on the dependent job while it is waiting on the blocker.
4. Cancel both jobs after validation.

This avoids interfering with GPU workloads and reduces scheduler races.

## Examples

User request:
"Check whether gflowd is healthy and show me the latest failed jobs."

Preferred flow:
1. `get_health`
2. `list_jobs` with a failure filter and explicit `limit`
3. `get_job` or `get_job_log` for the most relevant failures

User request:
"Test whether hold/release works."

Preferred flow:
1. Submit blocker test job
2. Submit dependent test job
3. `hold_job`
4. `update_job` if needed
5. `release_job`
6. `cancel_job` both test jobs

User request:
"Create a small parameter sweep with MCP."

Preferred flow:
1. `get_health`
2. Build the full set of test jobs with `gpus: 0` unless GPU behavior is part of the test
3. `submit_jobs_batch`
4. `list_jobs` or `get_job` to confirm the submitted IDs and states
5. `cancel_job` the test jobs after validation if they are no longer needed
