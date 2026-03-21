# Quick Start

This guide gets you running with gflow in a few minutes.

::: tip Before You Start
Make sure `tmux` is installed. If not, finish [Installation](./installation) first.
:::

## Optional: Initialize Configuration

Create a config file with sensible defaults:

```shell
gflowd init
```

## Step 1: Start the Scheduler

Start the daemon (runs inside a tmux session):

```shell
gflowd up
```

::: warning
If `gflowd up` fails, the most common cause is a missing `tmux` installation.
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

Then view output:

```shell
gjob log <job_id>
```

::: info
Use `gjob log <job_id>` after `gqueue` so you can inspect a specific completed or running job.
:::

## Step 4: Stop the Scheduler

```shell
gflowd down
```

## Next Steps

Now that you're familiar with the basics, explore:

- [Job Submission](../user-guide/job-submission) - Detailed job options
- [Time Limits](../user-guide/time-limits) - Managing job timeouts
- [Job Dependencies](../user-guide/job-dependencies) - Complex workflows
- [GPU Management](../user-guide/gpu-management) - GPU allocation
- [Configuration](../user-guide/configuration) - Defaults and system behavior
- [Quick Reference](../reference/quick-reference) - Command cheat sheet
