# gflow - A lightweight, single-node job scheduler

[![Documentation Status](https://img.shields.io/badge/docs-latest-brightgreen.svg?style=flat)](https://runqd.com)
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

`gflow` is a lightweight job scheduler for a single Linux machine. It gives you a Slurm-like workflow—submit, queue, inspect, cancel, and organize jobs—without deploying a cluster. It is especially useful on shared GPU workstations, lab servers, and small research boxes.

## Demo

[![asciicast](https://asciinema.org/a/ps79jhhtbo5cgJwO.svg)](https://asciinema.org/a/ps79jhhtbo5cgJwO)

## When gflow fits well

- You have one Linux machine instead of a full cluster.
- Multiple users or experiments need to share GPUs safely.
- You want job queues, dependencies, arrays, and time limits.
- You want a lighter alternative to Slurm for local or lab infrastructure.

## Core Features

- **Daemon-based scheduling**: `gflowd` keeps the queue, state, and resource allocation in one place.
- **GPU-aware execution**: schedule dedicated GPUs or shared GPUs with per-job VRAM limits.
- **Rich submission model**: submit commands or scripts with priorities, dependencies, arrays, and conda environments.
- **Time limits and lifecycle control**: prevent runaway jobs and manage hold, release, redo, and cancel actions.
- **tmux-backed execution and logs**: every job runs in its own session and streams output into persistent logs.
- **Automation hooks**: send webhook notifications when jobs start, finish, fail, or change state.

## CLI Overview

- `gflowd`: initialize config and manage the scheduler daemon.
- `gbatch`: submit commands or scripts.
- `gqueue`: inspect and filter jobs.
- `gjob`: show details, attach, hold/release, redo, and update jobs.
- `gcancel`: cancel one or more jobs.
- `gctl`: manage GPU visibility, concurrency limits, and reservations.
- `ginfo`: inspect scheduler and GPU status.
- `gstats`: view scheduler statistics.

## Installation

### Prerequisites

- Linux
- `tmux`
- NVIDIA drivers only if you want GPU scheduling features

### Install via PyPI (Recommended)

Use `uv`:

```bash
uv tool install runqd
```

Or `pipx`:

```bash
pipx install runqd
```

Or `pip`:

```bash
pip install runqd
```

Prebuilt binaries are available for Linux (`x86_64`, `arm64`).

### Install Nightly Build

```bash
pip install --index-url https://test.pypi.org/simple/ runqd
```

### Install via Cargo

```bash
cargo install gflow
```

Install directly from `main`:

```bash
cargo install --git https://github.com/AndPuQing/gflow.git --locked
```

### Build from Source

```bash
git clone https://github.com/AndPuQing/gflow.git
cd gflow
cargo build --release
```

Compiled binaries are placed in `target/release/`.

## Quick Start

1. **Initialize config** (optional but recommended):

   ```bash
   gflowd init
   ```

2. **Start the scheduler daemon**:

   ```bash
   gflowd up
   ```

3. **Submit a job**:

   ```bash
   cat > my_job.sh <<'EOF'
   #!/bin/bash
   echo "Starting job on GPU: $CUDA_VISIBLE_DEVICES"
   sleep 30
   echo "Job finished."
   EOF
   chmod +x my_job.sh
   gbatch --gpus 1 ./my_job.sh
   ```

4. **Inspect the queue**:

   ```bash
   gqueue
   ```

5. **Check details or stop the daemon when done**:

   ```bash
   ginfo
   gflowd down
   ```

## Common Workflow

```bash
gflowd up
ginfo
gbatch --gpus 1 --time 2:00:00 --name train python train.py
gqueue -f JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
gjob show <job_id>
gcancel <job_id>
```

## Documentation

- Website: [runqd.com](https://runqd.com)
- Installation: [runqd.com/getting-started/installation.html](https://runqd.com/getting-started/installation.html)
- Quick start: [runqd.com/getting-started/quick-start.html](https://runqd.com/getting-started/quick-start.html)
- Job submission: [runqd.com/user-guide/job-submission.html](https://runqd.com/user-guide/job-submission.html)
- Configuration: [runqd.com/user-guide/configuration.html](https://runqd.com/user-guide/configuration.html)
- Quick reference: [runqd.com/reference/quick-reference.html](https://runqd.com/reference/quick-reference.html)

## Star History

<a href="https://www.star-history.com/#AndPuQing/gflow&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
 </picture>
</a>

## Contributing

If you find a bug or want to propose an improvement, please open an [Issue](https://github.com/AndPuQing/gflow/issues) or submit a [Pull Request](https://github.com/AndPuQing/gflow/pulls).

## License

`gflow` is licensed under the MIT License. See [LICENSE](./LICENSE) for details.
