# gflow - 轻量级单节点任务调度器

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

`gflow` 是一个面向单台 Linux 机器的轻量级任务调度器。它提供类似 Slurm 的工作流：提交、排队、查看、取消和组织任务，但不需要部署完整集群。它特别适合共享 GPU 工作站、实验室服务器和小型研究环境。

## 演示

[![asciicast](https://asciinema.org/a/ps79jhhtbo5cgJwO.svg)](https://asciinema.org/a/ps79jhhtbo5cgJwO)

## 适用场景

- 只有一台 Linux 机器，而不是完整集群。
- 多个用户或实验需要安全地共享 GPU。
- 需要任务队列、依赖、数组任务和时间限制。
- 希望获得比 Slurm 更轻量的本地/实验室调度方案。

## 核心特性

- **守护进程调度**：`gflowd` 统一维护任务队列、状态和资源分配。
- **GPU 感知调度**：支持独占 GPU，也支持基于显存限制的共享 GPU 调度。
- **丰富的提交模型**：可提交命令或脚本，并支持优先级、依赖、任务数组和 Conda 环境。
- **时间限制与生命周期控制**：防止失控任务，并支持 hold、release、redo 和 cancel。
- **基于 tmux 的执行与日志**：每个任务在独立会话中运行，并将输出持续写入日志。
- **自动化集成**：在任务启动、完成、失败或状态变更时发送 Webhook 通知。

## CLI 概览

- `gflowd`：初始化配置并管理调度器守护进程。
- `gbatch`：提交命令或脚本。
- `gqueue`：查看并筛选任务。
- `gjob`：查看详情、attach、hold/release、redo 和 update。
- `gcancel`：取消一个或多个任务。
- `gctl`：管理 GPU 可见性、并发限制和预留。
- `ginfo`：查看调度器和 GPU 状态。
- `gstats`：查看调度统计信息。

## 安装

### 前置要求

- Linux
- `tmux`
- 仅在需要 GPU 调度时安装 NVIDIA 驱动

### 通过 PyPI 安装（推荐）

使用 `uv`：

```bash
uv tool install runqd
```

或使用 `pipx`：

```bash
pipx install runqd
```

或使用 `pip`：

```bash
pip install runqd
```

Linux `x86_64` 和 `arm64` 提供预构建二进制文件。

### 安装 Nightly 版本

```bash
pip install --index-url https://test.pypi.org/simple/ runqd
```

### 通过 Cargo 安装

```bash
cargo install gflow
```

直接安装 `main` 分支：

```bash
cargo install --git https://github.com/AndPuQing/gflow.git --locked
```

### 从源码构建

```bash
git clone https://github.com/AndPuQing/gflow.git
cd gflow
cargo build --release
```

编译后的二进制文件位于 `target/release/`。

## 快速开始

1. **初始化配置**（可选但推荐）：

   ```bash
   gflowd init
   ```

2. **启动调度器守护进程**：

   ```bash
   gflowd up
   ```

3. **提交任务**：

   ```bash
   cat > my_job.sh <<'EOF'
   #!/bin/bash
   echo "任务在 GPU 上启动：$CUDA_VISIBLE_DEVICES"
   sleep 30
   echo "任务完成。"
   EOF
   chmod +x my_job.sh
   gbatch --gpus 1 ./my_job.sh
   ```

4. **查看队列**：

   ```bash
   gqueue
   ```

5. **查看资源或在结束后停止守护进程**：

   ```bash
   ginfo
   gflowd down
   ```

## 常见工作流

```bash
gflowd up
ginfo
gbatch --gpus 1 --time 2:00:00 --name train python train.py
gqueue -f JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
gjob show <job_id>
gcancel <job_id>
```

## MCP

`gflow` 也可以作为本地 MCP 服务器运行，供 Claude Desktop、Cursor 等 AI 工具调用。

启动命令：

```bash
gflow mcp serve
```

Claude Desktop 的示例配置见 [examples/mcp/claude-desktop.json](./examples/mcp/claude-desktop.json)。这个模式默认面向本地使用：保持同一台机器上的 `gflowd` 处于运行状态，再由 MCP 服务器通过现有配置连接本地守护进程。

## 文档导航

- 文档站点：[runqd.com](https://runqd.com/zh-CN/)
- 安装：[runqd.com/zh-CN/getting-started/installation.html](https://runqd.com/zh-CN/getting-started/installation.html)
- 快速入门：[runqd.com/zh-CN/getting-started/quick-start.html](https://runqd.com/zh-CN/getting-started/quick-start.html)
- 任务提交：[runqd.com/zh-CN/user-guide/job-submission.html](https://runqd.com/zh-CN/user-guide/job-submission.html)
- 配置：[runqd.com/zh-CN/user-guide/configuration.html](https://runqd.com/zh-CN/user-guide/configuration.html)
- 命令速查：[runqd.com/zh-CN/reference/quick-reference.html](https://runqd.com/zh-CN/reference/quick-reference.html)

## Star 历史

<a href="https://www.star-history.com/#AndPuQing/gflow&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
 </picture>
</a>

## 贡献

如果你发现 Bug，或想提出改进建议，欢迎提交 [Issue](https://github.com/AndPuQing/gflow/issues) 或 [Pull Request](https://github.com/AndPuQing/gflow/pulls)。

## 许可证

`gflow` 采用 MIT 许可证。详见 [LICENSE](./LICENSE)。
