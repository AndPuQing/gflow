# Testing mdbook-cmdrun

This document demonstrates that all gflow commands in the documentation are executable and produce real output during the build process.

## Version Information

Check the gflow version:

```bash
$ gctl --version
<!-- cmdrun gctl --version -->
```

## Daemon Status

Check if the daemon is running:

```bash
$ gctl status
<!-- cmdrun gctl status -->
```

## Help Commands

### gctl Help

```bash
$ gctl --help
<!-- cmdrun gctl --help -->
```

### gbatch Help

```bash
$ gbatch --help
<!-- cmdrun gbatch --help -->
```

### gqueue Help

```bash
$ gqueue --help
<!-- cmdrun gqueue --help -->
```

### gcancel Help

```bash
$ gcancel --help
<!-- cmdrun gcancel --help -->
```

## Real Job Execution

Submit a simple test job:

```bash
$ gbatch --command "echo 'Hello from gflow!'; sleep 1; echo 'Job completed successfully'"
<!-- cmdrun gbatch --command "echo 'Hello from gflow!'; sleep 1; echo 'Job completed successfully'" -->
```

Wait for the job to start and check the queue:

```bash
$ sleep 2 && gqueue
<!-- cmdrun sleep 2 && gqueue -->
```

Wait for job completion and check again:

```bash
$ sleep 2 && gqueue
<!-- cmdrun sleep 2 && gqueue -->
```

## Job with Priority

Submit a job with custom priority:

```bash
$ gbatch --priority 50 --command "echo 'High priority job'"
<!-- cmdrun gbatch --priority 50 --command "echo 'High priority job'" -->
```

## Job with Time Limit

Submit a job with a time limit:

```bash
$ gbatch --time 30 --command "echo 'Job with 30-second limit'"
<!-- cmdrun gbatch --time 30 --command "echo 'Job with 30-second limit'" -->
```

## System Information

Show system info and GPU allocation:

```bash
$ gctl info
<!-- cmdrun gctl info -->
```

## Data Directory Contents

Show gflow data directory:

```bash
$ ls -la ~/.local/share/gflow/
<!-- cmdrun ls -la ~/.local/share/gflow/ -->
```

---

**Note**: All commands with `<!-- cmdrun ... -->` annotations are executed during `mdbook build`. This ensures that the documentation examples are always accurate and up-to-date with the current version of gflow.
