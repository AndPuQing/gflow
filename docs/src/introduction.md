# Introduction

Welcome to the **gflow** documentation! gflow is a lightweight, single-node job scheduler written in Rust, inspired by Slurm. It is designed for efficiently managing and scheduling tasks, especially on machines with GPU resources.

## What is gflow?

gflow provides a simple yet powerful way to:
- **Queue and schedule jobs** on a single machine with multiple GPUs
- **Manage dependencies** between jobs
- **Set priorities** for different tasks
- **Enforce time limits** to prevent runaway jobs
- **Monitor and control** job execution through intuitive CLI tools

## Who is gflow for?

gflow is perfect for:
- **ML/DL Researchers**: Training multiple models on a shared workstation
- **Data Scientists**: Running long experiments with proper resource allocation
- **Students**: Learning job scheduling concepts in a simplified environment
- **Developers**: Testing and debugging batch processing workflows
- **Anyone** who needs better control over job execution on a single powerful machine

## Key Features

### 🚀 Daemon-based Scheduling
A persistent daemon (`gflowd`) runs in the background, managing the job queue and automatically allocating resources.

### 📋 Rich Job Submission
Submit jobs with various options:
- GPU resource requests
- Job dependencies
- Priority levels
- Time limits
- Conda environment activation
- Job arrays for parallel tasks

### ⏱️ Time Limits
Set maximum runtime for jobs (similar to Slurm's `--time`) to prevent runaway processes:
```bash
gbatch --time 2:00:00 --command "python train.py"
```

### 🔗 Job Dependencies
Create complex workflows where jobs depend on others:
```bash
gbatch --depends-on 123 --command "python postprocess.py"
```

### 📊 Powerful Monitoring
Query and filter jobs with flexible options:
```bash
gqueue -s Running -f JOBID,NAME,TIME,TIMELIMIT
```

### 🖥️ tmux Integration
Every job runs in its own tmux session, allowing you to:
- Attach to running jobs
- View output in real-time
- Resume interrupted sessions
- Automatic output logging to files

## Quick Example

```bash
# Start the scheduler
$ gctl up

# Submit a training job with 1 GPU and 2-hour time limit
$ gbatch --gpus 1 --time 2:00:00 --command "python train.py"

# Check the job queue
$ gqueue
<!-- cmdrun gqueue -n 5 -->

# Watch jobs in real-time
$ watch gqueue

# Stop the scheduler
$ gctl down
```

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
│          gflowd Daemon            │
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

## Command Overview

gflow consists of five command-line tools:

| Command | Purpose | Similar to |
|---------|---------|------------|
| `gflowd` | Scheduler daemon | Slurm's `slurmctld` |
| `gctl` | Control the daemon | `systemctl` for slurm |
| `gbatch` | Submit jobs | Slurm's `sbatch` |
| `gqueue` | Query job queue | Slurm's `squeue` |
| `gcancel` | Cancel jobs | Slurm's `scancel` |

## Getting Started

Ready to dive in? Check out the [Installation Guide](./getting-started/installation.md) to get gflow up and running!

## Contributing

gflow is open source! Contributions are welcome:
- 🐛 [Report bugs](https://github.com/AndPuQing/gflow/issues)
- 💡 [Request features](https://github.com/AndPuQing/gflow/issues)
- 🔧 [Submit pull requests](https://github.com/AndPuQing/gflow/pulls)
- 📖 [Improve documentation](https://github.com/AndPuQing/gflow/edit/main/docs/)

## License

gflow is licensed under the MIT License. See [LICENSE](https://github.com/AndPuQing/gflow/blob/main/LICENSE) for details.

---

**Next**: [Installation Guide](./getting-started/installation.md)
