# gflow - 轻量级单节点任务调度器

[![Documentation Status](https://img.shields.io/badge/docs-latest-brightgreen.svg?style=flat)](https://andpuqing.github.io/gflow/)
[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/AndPuQing/gflow/ci.yml?style=flat-square&logo=github)](https://github.com/AndPuQing/gflow/actions/workflows/ci.yml)
[![PyPI - Version](https://img.shields.io/pypi/v/runqd?style=flat-square&logo=pypi)](https://pypi.org/project/runqd/)
[![TestPyPI - Version](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Ftest.pypi.org%2Fpypi%2Frunqd%2Fjson&query=%24.info.version&style=flat-square&logo=pypi&label=testpypi)](https://test.pypi.org/project/runqd/)
[![Crates.io Version](https://img.shields.io/crates/v/gflow?style=flat-square&logo=rust)](https://crates.io/crates/gflow)
[![Crates.io Downloads (recent)](https://img.shields.io/crates/dr/gflow?style=flat-square)](https://crates.io/crates/gflow)
[![dependency status](https://deps.rs/repo/github/AndPuQing/gflow/status.svg?style=flat-square)](https://deps.rs/repo/github/AndPuQing/gflow)
[![Crates.io License](https://img.shields.io/crates/l/gflow?style=flat-square)](https://crates.io/crates/gflow)
[![Crates.io Size](https://img.shields.io/crates/size/gflow?style=flat-square)](https://crates.io/crates/gflow)
[![Discord](https://img.shields.io/discord/1460169213149712415?style=flat-square)](https://discord.gg/wJRkDmYQrG)

[English](README.md) | 简体中文

`gflow` 是一个使用 Rust 编写的轻量级单节点任务调度器，灵感来源于 Slurm。它专为高效管理和调度任务而设计，特别适用于配备 GPU 资源的机器。

## 核心特性

- **守护进程调度**：持久化守护进程（`gflowd`）负责管理任务队列和资源分配。
- **丰富的任务提交选项**：通过 `gbatch` 命令支持依赖关系、优先级、任务数组和时间限制。
- **时间限制**：为任务设置最大运行时间（类似 Slurm 的 `--time`），防止失控进程。
- **服务与任务控制**：提供清晰的命令来检查调度器状态（`ginfo`）、查询任务队列（`gqueue`）和控制任务状态（`gcancel`）。
- **`tmux` 集成**：使用 `tmux` 实现稳健的后台任务执行和会话管理。
- **输出日志记录**：通过 `tmux pipe-pane` 自动捕获任务输出到日志文件。
- **简洁的命令行界面**：提供用户友好且功能强大的命令行工具集。

## 组件概览

`gflow` 套件包含多个命令行工具：

- `gflowd`：在后台运行的调度器守护进程，负责管理任务和资源。
- `ginfo`：显示调度器和 GPU 信息。
- `gbatch`：向调度器提交任务，类似 Slurm 的 `sbatch`。
- `gqueue`：列出和过滤队列中的任务，类似 Slurm 的 `squeue`。
- `gjob`：任务查看与控制（日志、attach、update、redo 等）。
- `gctl`：守护进程/运行时控制工具（例如 GPU 限制）。
- `gcancel`：取消任务。

## 安装

### 通过 PyPI 安装（推荐）

使用 `pipx` 安装 gflow（推荐用于 CLI 工具）：

```bash
pipx install runqd
```

或使用 `uv`：

```bash
uv tool install runqd
```

或使用 `pip`：

```bash
pip install runqd
```

这将为 Linux（x86_64、ARM64、ARMv7）安装预构建的二进制文件，支持 GNU 和 MUSL libc。

### 安装 Nightly 版本

如需体验最新开发版本，可从 TestPyPI 安装：

```bash
pip install --index-url https://test.pypi.org/simple/ runqd
```

### 通过 `cargo` 安装

```bash
cargo install gflow
```

#### `cargo install`（main 分支）
```bash
cargo install --git https://github.com/AndPuQing/gflow.git --locked
```

这将安装所有必需的二进制文件（`gflowd`、`ginfo`、`gbatch`、`gqueue`、`gcancel`、`gjob`、`gctl`）。

### 手动构建

1.  克隆仓库：
    ```bash
    git clone https://github.com/AndPuQing/gflow.git
    cd gflow
    ```

2.  构建项目：
    ```bash
    cargo build --release
    ```
    可执行文件将位于 `target/release/` 目录中。

## 快速开始

1.  **启动调度器守护进程**：
    ```bash
    gflowd up
    ```
    在专用终端或 `tmux` 会话中运行此命令并保持运行。您可以随时使用 `gflowd status` 检查其健康状态，并使用 `ginfo` 查看资源信息。

2.  **提交任务**：
    创建脚本 `my_job.sh`：
    ```sh
    #!/bin/bash
    echo "任务在 GPU 上启动：$CUDA_VISIBLE_DEVICES"
    sleep 30
    echo "任务完成。"
    ```
    使用 `gbatch` 提交：
    ```bash
    gbatch --gpus 1 ./my_job.sh
    ```

3.  **查看任务队列**：
    ```bash
    gqueue
    ```
    您也可以实时监控队列更新：`watch --color gqueue`。

4.  **停止调度器**：
    ```bash
    gflowd down
    ```
    这将关闭守护进程并清理 tmux 会话。

## 文档

- 在线文档：https://andpuqing.github.io/gflow/
- 安装：`docs/src/zh-CN/getting-started/installation.md`
- 快速入门：`docs/src/zh-CN/getting-started/quick-start.md`
- 任务提交：`docs/src/zh-CN/user-guide/job-submission.md`
- 时间限制：`docs/src/zh-CN/user-guide/time-limits.md`
- 配置：`docs/src/zh-CN/user-guide/configuration.md`
- 命令速查：`docs/src/zh-CN/reference/quick-reference.md`

## Star 历史

<a href="https://www.star-history.com/#AndPuQing/gflow&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=AndPuQing/gflow&type=date&legend=top-left" />
 </picture>
</a>

## 贡献

如果您发现任何错误或有功能请求，欢迎创建 [Issue](https://github.com/AndPuQing/gflow/issues) 并通过提交 [Pull Request](https://github.com/AndPuQing/gflow/pulls) 来贡献代码。

## 许可证

`gflow` 采用 MIT 许可证。详情请参阅 [LICENSE](./LICENSE)。
