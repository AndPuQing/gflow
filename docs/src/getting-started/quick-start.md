# Quick Start

This guide gets you running with gflow in a few minutes.

## 1) Start the Scheduler

Start the daemon (runs inside a tmux session):

```shell
gflowd up
```

If this fails, make sure `tmux` is installed (see [Installation](./installation)).

Check status:

```shell
gflowd status
```

Verify the client can reach it:

```shell
ginfo
```

## 2) Submit a Job

```shell
gbatch echo 'Hello from gflow!'
```

## 3) Check Queue and Logs

```shell
gqueue
```

Then view output:

```shell
gjob log <job_id>
```

## 4) Stop the Scheduler

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

---

**Previous**: [Installation](./installation) | **Next**: [Job Submission](../user-guide/job-submission)
