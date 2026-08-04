# gflow

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

English | [简体中文](README_CN.md)

`gflow` is a lightweight scheduler for a single Linux machine. It brings a Slurm-like workflow to shared GPU workstations and lab servers without cluster setup.

[![asciicast](https://asciinema.org/a/777578.svg)](https://asciinema.org/a/777578)

## Why gflow

- Queue and run jobs on one machine.
- Submit commands or scripts with GPUs, time limits, dependencies, arrays, and priorities.
- Inspect, attach, cancel, and recover jobs with a small CLI.

## Install

Requirements: Linux, `tmux`, and NVIDIA drivers only if you need GPU scheduling.

Install with Python tooling:

```bash
uv tool install runqd
# or
pipx install runqd
# or
pip install runqd
```

Install with Cargo:

```bash
cargo install gflow
```

Nightly build:

```bash
pip install --index-url https://test.pypi.org/simple/ runqd
```

## Quick Start

```bash
gflowd init
gflowd up
gbatch --gpus 1 --name demo bash -lc 'echo "hello from gflow"; sleep 30'
gqueue
gjob show <job_id>
gflowd down
```

## MCP

`gflow` can also run as a local MCP server for Claude Desktop, Claude Code, Codex, Cursor, and similar tools:

```bash
gflow mcp serve
```

Keep `gflowd` running on the same machine. MCP clients start `gflow mcp serve` as a local stdio server.

Claude Desktop example:

- [examples/mcp/claude-desktop.json](./examples/mcp/claude-desktop.json)

Claude Code:

```bash
claude mcp add --scope user gflow -- gflow mcp serve
```

Codex:

```bash
codex mcp add gflow -- gflow mcp serve
```

Or via `~/.codex/config.toml`:

```toml
[mcp_servers.gflow]
command = "gflow"
args = ["mcp", "serve"]
```

If `gflow` is not on your `PATH`, replace it with the absolute binary path.

## Documentation

Most usage details live in the docs:

- [Quick start](https://runqd.com/getting-started/quick-start.html)
- [Installation](https://runqd.com/getting-started/installation.html)
- [User guide](https://runqd.com/user-guide/job-submission.html)
- [Command reference](https://runqd.com/reference/quick-reference.html)

## Star History

<a href="https://www.star-history.com/?repos=andpuqing%2Fgflow&type=timeline&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=andpuqing/gflow&type=timeline&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=andpuqing/gflow&type=timeline&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=andpuqing/gflow&type=timeline&legend=top-left" />
 </picture>
</a>

## Contact Us

For coordinating contributions and development, please use [Slack](https://join.slack.com/t/runqd/shared_invite/zt-3vddqdds0-zfMwbzCNizQFluWglMQi0w)

## Contributing

Please open an [Issue](https://github.com/AndPuQing/gflow/issues) or [Pull Request](https://github.com/AndPuQing/gflow/pulls).

## License

MIT. See [LICENSE](./LICENSE).
