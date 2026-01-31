# gflow - A lightweight, single-node job scheduler

[![Documentation Status](https://img.shields.io/badge/docs-latest-brightgreen.svg?style=flat)](https://andpuqing.github.io/gflow/)
[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/AndPuQing/gflow/ci.yml?style=flat-square&logo=github)](https://github.com/AndPuQing/gflow/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/AndPuQing/gflow/branch/main/graph/badge.svg?style=flat-square)](https://codecov.io/gh/AndPuQing/gflow)
[![PyPI - Version](https://img.shields.io/pypi/v/runqd?style=flat-square&logo=pypi)](https://pypi.org/project/runqd/)
[![TestPyPI - Version](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Ftest.pypi.org%2Fpypi%2Frunqd%2Fjson&query=%24.info.version&style=flat-square&logo=pypi&label=testpypi)](https://test.pypi.org/project/runqd/)
[![Crates.io Version](https://img.shields.io/crates/v/gflow?style=flat-square&logo=rust)](https://crates.io/crates/gflow)
[![PyPI - Downloads](https://img.shields.io/pypi/dm/runqd?style=flat-square)](https://pypi.org/project/runqd/)
[![dependency status](https://deps.rs/repo/github/AndPuQing/gflow/status.svg?style=flat-square)](https://deps.rs/repo/github/AndPuQing/gflow)
[![Crates.io License](https://img.shields.io/crates/l/gflow?style=flat-square)](https://crates.io/crates/gflow)
[![Crates.io Size](https://img.shields.io/crates/size/gflow?style=flat-square)](https://crates.io/crates/gflow)
[![Discord](https://img.shields.io/discord/1460169213149712415?style=flat-square)](https://discord.gg/wJRkDmYQrG)

English | [简体中文](README_CN.md)

`gflow` is a lightweight, single-node job scheduler written in Rust, inspired by Slurm. It is designed for efficiently managing and scheduling tasks, especially on machines with GPU resources.

## Core Features

- **Daemon-based Scheduling**: A persistent daemon (`gflowd`) manages the job queue and resource allocation.
- **Rich Job Submission**: Supports dependencies, priorities, job arrays, and time limits via the `gbatch` command.
- **Time Limits**: Set maximum runtime for jobs (similar to Slurm's `--time`) to prevent runaway processes.
- **Service and Job Control**: Provides clear commands to inspect the scheduler state (`ginfo`), query the job queue (`gqueue`), and control job states (`gcancel`).
- **`tmux` Integration**: Uses `tmux` for robust, background task execution and session management.
- **Output Logging**: Automatic capture of job output to log files via `tmux pipe-pane`.
- **Simple Command-Line Interface**: Offers a user-friendly and powerful set of command-line tools.

## Component Overview

The `gflow` suite consists of several command-line tools:

- `gflowd`: The scheduler daemon that runs in the background, managing jobs and resources.
- `ginfo`: Displays scheduler and GPU information.
- `gbatch`: Submits jobs to the scheduler, similar to Slurm's `sbatch`.
- `gqueue`: Lists and filters jobs in the queue, similar to Slurm's `squeue`.
- `gjob`: Job inspection and control (logs, attach, update, redo, ...).
- `gctl`: Daemon/runtime control utilities (e.g. GPU restriction).
- `gcancel`: Cancels jobs.

## Installation

### Install via PyPI (Recommended)

Install gflow using `pipx` (recommended for CLI tools):

```bash
pipx install runqd
```

Or using `uv`:

```bash
uv tool install runqd
```

Or using `pip`:

```bash
pip install runqd
```

This will install pre-built binaries for Linux (x86_64, ARM64, ARMv7) with both GNU and MUSL libc support.

### Install Nightly Build

To try the latest development version, install from TestPyPI:

```bash
pip install --index-url https://test.pypi.org/simple/ runqd
```

### Install via `cargo`

```bash
cargo install gflow
```

#### `cargo install`(main branch)
```bash
cargo install --git https://github.com/AndPuQing/gflow.git --locked
```

This will install all the necessary binaries (`gflowd`, `ginfo`, `gbatch`, `gqueue`, `gcancel`, `gjob`, `gctl`).

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
    gflowd up
    ```
    Run this in a dedicated terminal or `tmux` session and leave it running. You can check its health at any time with `gflowd status` and inspect resources with `ginfo`.

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
    You can also watch the queue update live: `watch --color gqueue`.

4.  **Stop the scheduler**:
    ```bash
    gflowd down
    ```
    This shuts down the daemon and cleans up the tmux session.

## Documentation

- Website: https://andpuqing.github.io/gflow/
- Installation: `docs/src/getting-started/installation.md`
- Quick start: `docs/src/getting-started/quick-start.md`
- Job submission: `docs/src/user-guide/job-submission.md`
- Time limits: `docs/src/user-guide/time-limits.md`
- Configuration: `docs/src/user-guide/configuration.md`
- Command quick reference: `docs/src/reference/quick-reference.md`

## Star History

<a href="https://www.star-history.com/#AndPuQing/gflow&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
 </picture>
</a>

## Contributing

If you find any bugs or have feature requests, feel free to create an [Issue](https://github.com/AndPuQing/gflow/issues) and contribute by submitting [Pull Requests](https://github.com/AndPuQing/gflow/pulls).

## License

`gflow` is licensed under the MIT License. See [LICENSE](./LICENSE) for more details.
