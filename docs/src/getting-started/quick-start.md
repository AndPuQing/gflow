# Quick Start

Run through the smallest gflow workflow with the commands below.

::: tip Before You Start
Make sure `tmux` is installed. If not, finish [Installation](./installation) first.
:::

## Optional: Initialize Configuration

Create a default config:

```shell
gflowd init
```

## Step 1: Start the Scheduler

Start the daemon:

```shell
gflowd start
```

::: tip
`gflowd start` hosts the daemon via the systemd user service when installed,
otherwise in tmux, otherwise as a direct process — no need to worry about how.
:::

::: warning
If `gflowd start` fails, check `tmux` first (or install the systemd user
service with `gflowd service install`).
:::

Check status:

```shell
gflowd status
```

Verify the client can reach it:

```shell
ginfo
```

## Step 2: Submit a Job

```shell
gbatch echo 'Hello from gflow!'
```

## Step 3: Check Queue and Logs

```shell
gqueue
```

Then read the logs:

```shell
gjob log <job_id>
```

::: info
Use `gqueue` first to find the job ID.
:::

## Step 4: Stop the Scheduler

```shell
gflowd stop
```

## Next Steps

Next:

- [Job Submission](../user-guide/job-submission)
- [Time Limits](../user-guide/time-limits)
- [Job Dependencies](../user-guide/job-dependencies)
- [GPU Management](../user-guide/gpu-management)
- [Configuration](../user-guide/configuration)
- [Multi-User Usage](../user-guide/multi-user)
- [Quick Reference](../reference/quick-reference)
