# Testing mdbook-cmdrun

This document demonstrates that all gflow commands in the documentation are executable and produce real output during the build process.

## Version Information

Check the gflow version:

```bash
<!-- cmdrun gctl --version -->
```

## Daemon Status

Check if the daemon is running:

```bash
<!-- cmdrun gctl status -->
```

## Help Commands

### gctl Help

```bash
<!-- cmdrun gctl --help -->
```

### gbatch Help

```bash
<!-- cmdrun gbatch --help -->
```

### gqueue Help

```bash
<!-- cmdrun gqueue --help -->
```

### gcancel Help

```bash
<!-- cmdrun gcancel --help -->
```

## Real Job Execution

Submit a simple test job:

```bash
<!-- cmdrun gbatch --command "echo 'Hello from gflow!'; sleep 1; echo 'Job completed successfully'" -->
```

Wait for the job to start:

```bash
<!-- cmdrun sleep 2 -->
```

Check the job queue:

```bash
<!-- cmdrun gqueue -->
```

Wait for job completion:

```bash
<!-- cmdrun sleep 2 -->
```

Check queue again (job should be completed):

```bash
<!-- cmdrun gqueue -->
```

## Job with Priority

Submit a job with custom priority:

```bash
<!-- cmdrun gbatch --priority 50 --command "echo 'High priority job'" -->
```

## Job with Time Limit

Submit a job with a time limit:

```bash
<!-- cmdrun gbatch --time 30 --command "echo 'Job with 30-second limit'" -->
```

## System Information

Show system info and GPU allocation:

```bash
<!-- cmdrun gctl info -->
```

## Data Directory Contents

Show gflow data directory:

```bash
<!-- cmdrun ls -la ~/.local/share/gflow/ -->
```

---

**Note**: All commands with `<!-- cmdrun ... -->` annotations are executed during `mdbook build`. This ensures that the documentation examples are always accurate and up-to-date with the current version of gflow.
