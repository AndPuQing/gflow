# gflow Documentation

Welcome to the gflow documentation! This directory contains detailed guides and references for using gflow effectively.

## Documentation Index

### User Guides

- **[Time Limits](TIME_LIMITS.md)** - Complete guide to setting and managing job time limits
  - Time format specifications (`HH:MM:SS`, `MM:SS`, `MM`)
  - Timeout behavior and enforcement
  - Best practices and examples
  - Troubleshooting common issues

### Quick Links

- [Main README](../README.md) - Getting started and basic usage
- [Contributing Guidelines](../CONTRIBUTING.md) - How to contribute to gflow
- [Code of Conduct](../CODE_OF_CONDUCT.md) - Community guidelines

## Quick Reference

### Common Commands

```bash
# Start/stop daemon
gctl start
gctl stop
gctl status

# Submit jobs
gbatch --command "python train.py"
gbatch --time 2:00:00 --gpus 1 --command "python train.py"
gbatch my_script.sh

# Query jobs
gqueue
gqueue -s Running
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT

# Cancel jobs
gcancel <job_id>
```

### Job Submission Options

| Option | Short | Description | Example |
|--------|-------|-------------|---------|
| `--command` | | Command to run | `--command "python train.py"` |
| `--gpus` | `-g` | Number of GPUs | `--gpus 2` |
| `--time` | `-t` | Time limit | `--time 2:00:00` |
| `--priority` | | Job priority (0-255) | `--priority 100` |
| `--depends-on` | | Dependency job ID | `--depends-on 123` |
| `--array` | | Job array spec | `--array 1-10` |
| `--conda-env` | `-c` | Conda environment | `--conda-env myenv` |

### Job States

| Code | State | Description |
|------|-------|-------------|
| `PD` | Queued | Waiting to run |
| `R` | Running | Currently executing |
| `CD` | Finished | Completed successfully |
| `F` | Failed | Exited with error |
| `CA` | Cancelled | Manually cancelled |
| `TO` | Timeout | Exceeded time limit |

### Time Limit Formats

| Format | Example | Duration |
|--------|---------|----------|
| `HH:MM:SS` | `2:30:00` | 2 hours 30 minutes |
| `MM:SS` | `45:30` | 45 minutes 30 seconds |
| `MM` | `30` | 30 minutes |

## Getting Help

- **Issues**: Report bugs at [GitHub Issues](https://github.com/AndPuQing/gflow/issues)
- **Discussions**: Ask questions in [GitHub Discussions](https://github.com/AndPuQing/gflow/discussions)
- **Pull Requests**: Contribute improvements via [Pull Requests](https://github.com/AndPuQing/gflow/pulls)

## Configuration

Default configuration locations:
- **Config file**: `~/.config/gflow/gflowd.toml`
- **State file**: `~/.local/share/gflow/state.json`
- **Log files**: `~/.local/share/gflow/logs/<job_id>.log`

## Architecture Overview

```
┌──────────────────────────────────┐
│          User Commands           │
│ (gbatch, gqueue, gcancel, gctl)  │
└────────────────┬─────────────────┘
                 │
                 │ HTTP API
                 ▼
┌──────────────────────────────────┐
│          gflowd Daemon           │
│  ┌────────────────────────────┐  │
│  │ Scheduler (5s interval)    │  │
│  │ - Check dependencies       │  │
│  │ - Check GPU availability   │  │
│  │ - Check timeouts           │  │
│  │ - Assign jobs to resources │  │
│  └────────────────────────────┘  │
└────────────────┬─────────────────┘
                 │
                 │ TmuxExecutor
                 ▼
 ┌─────────────────────────────────┐
 │       Tmux Sessions (Jobs)      │
 │  ┌───────┐ ┌───────┐ ┌───────┐  │
 │  │ Job 1 │ │ Job 2 │ │ Job 3 │  │
 │  │       │ │       │ │       │  │
 │  │(GPU 0)│ │(NoGPU)│ │(GPU 1)│  │
 │  └──┬────┘ └───┬───┘ └───┬───┘  │
 │     │          │         │      │
 │     └──────────┴─────────┘      │
 │        pipe-pane logging        │
 └───────────────┬─────────────────┘
                 │
                 ▼
     ~/.local/share/gflow/logs
```

## Contributing Documentation

To add new documentation:

1. Create a new `.md` file in the `docs/` directory
2. Add it to the index in this README
3. Link to it from the main README if appropriate
4. Follow the existing documentation style:
   - Clear headings and sections
   - Code examples with explanations
   - Tables for reference information
   - FAQ sections for common questions

## License

gflow is licensed under the MIT License. See [LICENSE](../LICENSE) for details.
