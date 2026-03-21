# 安装

按下面步骤安装 gflow。

::: info 包名说明
Python 包名是 `runqd`。安装后会提供 `gflowd`、`gbatch`、`gqueue`、`gjob`、`ginfo`、`gcancel` 和 `gctl` 等命令。
:::

## 前置要求

- **操作系统**：Linux
- **tmux**：必需
- **NVIDIA GPU / 驱动**：仅在需要 GPU 调度时必需

### 安装前置要求

::: code-group
```bash [Ubuntu/Debian]
# 安装 tmux
sudo apt-get update
sudo apt-get install tmux
```

```bash [Fedora/RHEL]
# 安装 tmux
sudo dnf install tmux
```
:::

## 安装方法

### 方法 1：通过 PyPI 安装

推荐用 `uv` 或 `pipx` 安装 CLI：

::: code-group
```bash [uv]
uv tool install runqd
```

```bash [pipx]
pipx install runqd
```

```bash [pip]
pip install runqd
```
:::

Linux `x86_64` 和 `ARM64` 可直接安装预构建二进制。

### 安装 Nightly 版本

需要最新开发版本时，可从 TestPyPI 安装：

::: code-group
```bash [uv]
uv tool install --index https://test.pypi.org/simple/ runqd
```

```bash [pipx]
pipx install --index-url https://test.pypi.org/simple/ runqd
```

```bash [pip]
pip install --index-url https://test.pypi.org/simple/ runqd
```
:::

### 方法 2：通过 Cargo 安装

从 crates.io 构建并安装：

::: code-group
```bash [crates.io]
cargo install gflow
```

```bash [main branch]
cargo install --git https://github.com/AndPuQing/gflow.git --locked
```
:::

二进制会安装到 `~/.cargo/bin/`，请确保它在 `PATH` 中。

### 方法 3：从源代码构建

需要从源码构建时：

1. 克隆仓库：
   ```bash
   git clone https://github.com/AndPuQing/gflow.git
   cd gflow
   ```

2. 构建项目：
   ```bash
   cargo build --release
   ```

   可执行文件位于 `target/release/`。

3. 安装到系统（可选）：
   ```bash
   cargo install --path .
   ```

## 验证安装

安装后先检查命令和版本：

```bash
# 检查命令
which gflowd ginfo gbatch gqueue gcancel

# 检查版本
gflowd --version
```

::: tip
命令可执行且 `gflowd --version` 有输出即可。
:::

## 运行检查

### 1. 检查 tmux
```bash
tmux new-session -d -s test
tmux has-session -t test && echo "tmux ok"
tmux kill-session -t test
```

### 2. 检查守护进程和 GPU（可选）

如果有 NVIDIA GPU，可以顺手验证是否被检测到：

```bash
# 可选：生成默认配置
gflowd init

# 启动守护进程
gflowd up

# 检查状态
gflowd status
```

查看系统信息和 GPU：
```bash
ginfo
```

如果检测成功，输出里会包含 GPU 信息。

## 文件位置

gflow 默认使用以下目录：

| 位置 | 用途 |
|----------|---------|
| `~/.config/gflow/gflow.toml` | 配置文件（可选） |
| `~/.local/share/gflow/state.msgpack` | 持久化任务状态（旧版本的 `state.json` 仍可读取） |
| `~/.local/share/gflow/logs/` | 任务输出日志 |

## 故障排除

::: details 问题：找不到命令

如果安装后提示“找不到命令”：

1. 检查 `~/.cargo/bin` 是否在 `PATH` 中：
   ```bash
   echo $PATH | grep -o ~/.cargo/bin
   ```

2. 如果缺失，添加到 `~/.bashrc` 或 `~/.zshrc`：
   ```bash
   export PATH="$HOME/.cargo/bin:$PATH"
   ```

3. 重新加载 shell：
   ```bash
   source ~/.bashrc  # 或 ~/.zshrc
   ```
:::

::: details 问题：未检测到 GPU

1. 检查 NVIDIA 驱动：
   ```bash
   nvidia-smi
   ```

2. 检查 NVML：
   ```bash
   ldconfig -p | grep libnvidia-ml
   ```

3. 如果检测失败，gflow 仍可用于 CPU 任务。
:::

## 更新 gflow

::: details 如果通过 Cargo 安装
```bash
cargo install gflow --force
```
:::

::: details 如果从源代码构建
```bash
cd gflow
git pull
cargo build --release
cargo install --path . --force
```
:::

## 卸载

要删除 gflow：

::: warning
下面可选的数据清理命令会永久删除本地调度状态和日志。
:::

```bash
# 首先停止守护进程
gflowd down

# 卸载二进制文件
cargo uninstall gflow

# 删除配置和数据（可选）
rm -rf ~/.config/gflow
rm -rf ~/.local/share/gflow
```

## 下一步

安装完成后，继续看[快速入门](./quick-start)。
