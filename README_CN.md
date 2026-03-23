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

[English](README.md) | 简体中文

`gflow` 是一个面向单台 Linux 机器的轻量级任务调度器。它为共享 GPU 工作站和实验室服务器提供接近 Slurm 的工作流，但不需要部署集群。

[![asciicast](https://asciinema.org/a/777578.svg)](https://asciinema.org/a/777578)


## 为什么用 gflow

- 在一台机器上完成排队和调度。
- 提交命令或脚本，并声明 GPU、时间限制、依赖、数组任务和优先级。
- 用精简 CLI 查看、attach、取消和恢复任务。

## 安装

前置要求：Linux、`tmux`，以及可选的 NVIDIA 驱动（仅在需要 GPU 调度时）。

使用 Python 工具安装：

```bash
uv tool install runqd
# 或
pipx install runqd
# 或
pip install runqd
```

使用 Cargo 安装：

```bash
cargo install gflow
```

安装 Nightly 版本：

```bash
pip install --index-url https://test.pypi.org/simple/ runqd
```

## 快速开始

```bash
gflowd init
gflowd up
gbatch --gpus 1 --name demo bash -lc 'echo "hello from gflow"; sleep 30'
gqueue
gjob show <job_id>
gflowd down
```

## MCP

`gflow` 也可以作为本地 MCP 服务器运行，供 Claude Desktop、Claude Code、Codex、Cursor 等工具调用：

```bash
gflow mcp serve
```

建议让同一台机器上的 `gflowd` 持续运行。MCP 客户端会按配置拉起本地 `stdio` server。

Claude Desktop 示例配置：

- [examples/mcp/claude-desktop.json](./examples/mcp/claude-desktop.json)

Claude Code：

```bash
claude mcp add --scope user gflow -- gflow mcp serve
```

Codex：

```bash
codex mcp add gflow -- gflow mcp serve
```

也可以直接写入 `~/.codex/config.toml`：

```toml
[mcp_servers.gflow]
command = "gflow"
args = ["mcp", "serve"]
```

如果 `gflow` 不在 `PATH` 中，请改成二进制的绝对路径。

## 文档

更完整的内容见文档站：

- [快速开始](https://runqd.com/zh-CN/getting-started/quick-start.html)
- [安装指南](https://runqd.com/zh-CN/getting-started/installation.html)
- [用户指南](https://runqd.com/zh-CN/user-guide/job-submission.html)
- [命令速查](https://runqd.com/zh-CN/reference/quick-reference.html)

## 贡献

欢迎提交 [Issue](https://github.com/AndPuQing/gflow/issues) 或 [Pull Request](https://github.com/AndPuQing/gflow/pulls)。

## 许可证

MIT，详见 [LICENSE](./LICENSE)。
