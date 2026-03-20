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
[![Discord](https://img.shields.io/discord/1460169213149712415?style=flat-square)](https://discord.gg/wJRkDmYQrG)

English | [简体中文](README_CN.md)

`gflow` is a lightweight job scheduler for a single Linux machine. It gives you a Slurm-like workflow for shared GPU workstations, lab servers, and small research boxes without deploying a full cluster.

## Why gflow

- Queue and manage jobs on one machine with a daemon-backed scheduler.
- Submit commands or scripts with GPUs, time limits, dependencies, arrays, and priorities.
- Inspect, attach, cancel, and recover jobs through a small CLI toolset.

## Install

Requirements:

- Linux
- `tmux`
- NVIDIA drivers only if you need GPU scheduling

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

`gflow` can also run as a local MCP server for Claude Desktop, Claude Code, Codex, Cursor, and similar tools. Use the following command as the MCP server entry in your client configuration:

```bash
gflow mcp serve
```

Keep `gflowd` running on the same machine and let the MCP server connect through the local config. MCP clients typically launch local stdio servers using the configured command and arguments.

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

Most detailed usage now lives in the docs:

- [Quick start](https://runqd.com/getting-started/quick-start.html)
- [Installation](https://runqd.com/getting-started/installation.html)
- [User guide](https://runqd.com/user-guide/job-submission.html)
- [Command reference](https://runqd.com/reference/quick-reference.html)

## Contributing

Please open an [Issue](https://github.com/AndPuQing/gflow/issues) or [Pull Request](https://github.com/AndPuQing/gflow/pulls).

## License

MIT. See [LICENSE](./LICENSE).
