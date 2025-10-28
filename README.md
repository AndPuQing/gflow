# gflow - A lightweight, single-node job scheduler

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/AndPuQing/gflow/ci.yml?style=flat-square&logo=github)
![Crates.io Version](https://img.shields.io/crates/v/gflow?style=flat-square&logo=rust)
![Crates.io Downloads (recent)](https://img.shields.io/crates/dr/gflow?style=flat-square)
[![dependency status](https://deps.rs/repo/github/AndPuQing/gflow/status.svg?style=flat-square)](https://deps.rs/repo/github/AndPuQing/gflow)
![Crates.io License](https://img.shields.io/crates/l/gflow?style=flat-square) ![Crates.io Size](https://img.shields.io/crates/size/gflow?style=flat-square)

`gflow` is a lightweight, single-node job scheduler written in Rust, inspired by Slurm. It is designed for efficiently managing and scheduling tasks, especially on machines with GPU resources.

## Snapshot

![gflow](./assets/Sniapaste.png)

## Core Features

- **Daemon-based Scheduling**: A persistent daemon (`gflowd`) manages the job queue and resource allocation.
- **Rich Job Submission**: Supports dependencies, priorities, job arrays, and time limits via the `gbatch` command.
- **Time Limits**: Set maximum runtime for jobs (similar to Slurm's `--time`) to prevent runaway processes.
- **Service and Job Control**: Provides clear commands to manage the scheduler daemon (`gctl`), query the job queue (`gqueue`), and control job states (`gcancel`).
- **`tmux` Integration**: Uses `tmux` for robust, background task execution and session management.
- **Output Logging**: Automatic capture of job output to log files via `tmux pipe-pane`.
- **Simple Command-Line Interface**: Offers a user-friendly and powerful set of command-line tools.

## Component Overview

The `gflow` suite consists of several command-line tools:

- `gflowd`: The scheduler daemon that runs in the background, managing jobs and resources.
- `gctl`: Controls the daemon (`gflowd`) and displays system information.
- `gbatch`: Submits jobs to the scheduler, similar to Slurm's `sbatch`.
- `gqueue`: Lists and filters jobs in the queue, similar to Slurm's `squeue`.
- `gcancel`: Cancels jobs and manages job states (internal use).

## Installation

### Install via `cargo` (Recommended)

```bash
cargo install gflow
```
This will install all the necessary binaries (`gflowd`, `gctl`, `gbatch`, `gqueue`, `gcancel`).

### Build Manually

1.  Clone the repository:
    ```bash
    git clone https://github.com/AndPuQing/gflow.git
    cd gflow
    ```

2.  Build the project:
    ```bash
    cargo build --release
    ```
    The executables will be available in the `target/release/` directory.

## Quick Start

1.  **Start the scheduler daemon**:
    ```bash
    gctl up
    ```
    You can check its status with `gctl status`.

2.  **Submit a job**:
    Create a script `my_job.sh`:
    ```sh
    #!/bin/bash
    echo "Starting job on GPU: $CUDA_VISIBLE_DEVICES"
    sleep 30
    echo "Job finished."
    ```
    Submit it using `gbatch`:
    ```bash
    gbatch --gpus 1 ./my_job.sh
    ```

3.  **Check the job queue**:
    ```bash
    gqueue
    ```
    You can also watch the queue update live: `watch gqueue`.

4.  **Stop the scheduler**:
    ```bash
    gctl down
    ```

## Usage Guide

### Submitting Jobs with `gbatch`

`gbatch` provides flexible options for job submission.

- **Submit a command directly**:
  ```bash
  gbatch --gpus 1 --command "python train.py --epochs 10"
  ```

- **Set a job name and priority**:
  ```bash
  gbatch --gpus 1 --name "training-run-1" --priority 10 ./my_job.sh
  ```

- **Create a job that depends on another**:
  ```bash
  # First job
  gbatch --gpus 1 --name "job1" ./job1.sh
  # Get job ID from gqueue, e.g., 123

  # Second job depends on the first
  gbatch --gpus 1 --name "job2" --depends-on 123 ./job2.sh
  ```

- **Set a time limit for a job**:
  ```bash
  # 30-minute limit
  gbatch --time 30 --command "python train.py"

  # 2-hour limit (HH:MM:SS format)
  gbatch --time 2:00:00 --command "python long_training.py"

  # 5 minutes 30 seconds
  gbatch --time 5:30 --command "python quick_task.py"
  ```

  See [docs/TIME_LIMITS.md](docs/TIME_LIMITS.md) for detailed documentation on time limits.

### Querying Jobs with `gqueue`

`gqueue` allows you to filter and format the job list.

- **Filter by job state**:
  ```bash
  gqueue --states Running,Queued
  ```

- **Filter by job ID or name**:
  ```bash
  gqueue --jobs 123,124
  gqueue --names "training-run-1"
  ```

- **Customize output format**:
  ```bash
  gqueue --format "ID,Name,State,GPUs"
  ```

## Configuration

Configuration for `gflowd` can be customized. The default configuration file is located at `~/.config/gflow/gflowd.toml`.

## Contributing

If you find any bugs or have feature requests, feel free to create an [Issue](https://github.com/AndPuQing/gflow/issues) and contribute by submitting [Pull Requests](https://github.com/AndPuQing/gflow/pulls).

## License

`gflow` is licensed under the MIT License. See [LICENSE](./LICENSE) for more details.
