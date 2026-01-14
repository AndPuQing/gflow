# 安装

本指南将帮助您在系统上安装 gflow。

## 前置要求

- **操作系统**：Linux（在 Ubuntu 20.04+ 上测试）
- **Rust**：版本 1.70+（用于从源代码构建）
- **tmux**：任务执行所必需
- **NVIDIA GPU**（可选）：用于 GPU 任务调度
- **NVIDIA 驱动程序**（可选）：如果使用 GPU 功能

### 安装前置要求

#### Ubuntu/Debian
```bash
# 安装 tmux
sudo apt-get update
sudo apt-get install tmux

# 安装 Rust（如果从源代码构建）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### Fedora/RHEL
```bash
# 安装 tmux
sudo dnf install tmux

# 安装 Rust（如果从源代码构建）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## 安装方法

### 方法 1：使用预构建二进制文件快速安装（推荐）

这是在 Linux x86_64 上安装 gflow 的最快方法：

**使用我们的全球 CDN**（更快，在 GitHub 受限地区也可用）：
```bash
curl -fsSL https://gflow-releases.puqing.work/install.sh | sh
```

**或使用 GitHub**：
```bash
curl -fsSL https://raw.githubusercontent.com/AndPuQing/gflow/main/install.sh | sh
```

安装程序会自动：
- 首先尝试 CDN，如果需要则回退到 GitHub
- 下载最新版本的二进制文件
- 安装到 `~/.cargo/bin/`（可通过 `GFLOW_INSTALL_DIR` 自定义）
- 安装所有二进制文件：`gflowd`、`ginfo`、`gbatch`、`gqueue`、`gcancel`、`gjob`、`gctl`

**自定义安装目录**：
```bash
curl -fsSL https://gflow-releases.puqing.work/install.sh | GFLOW_INSTALL_DIR=/usr/local/bin sh
```

### 方法 2：通过 Cargo 安装

从 crates.io 构建并安装：

```bash
cargo install gflow
```

这将编译并安装所有二进制文件到 `~/.cargo/bin/`，该目录应该在您的 `PATH` 中。

### 方法 3：从源代码构建

如果您想从最新的源代码构建：

1. **克隆仓库**：
   ```bash
   git clone https://github.com/AndPuQing/gflow.git
   cd gflow
   ```

2. **构建项目**：
   ```bash
   cargo build --release
   ```

   可执行文件将位于 `target/release/` 中。

3. **安装到系统**（可选）：
   ```bash
   cargo install --path .
   ```

### 方法 4：从 CDN 或 GitHub 手动下载

直接下载预构建的二进制文件：

**从 CDN**：
- 最新版本：`https://gflow-releases.puqing.work/releases/latest/`
- 特定版本：`https://gflow-releases.puqing.work/releases/v0.4.5/`（替换为所需版本）

**从 GitHub**：
- 访问 [Releases 页面](https://github.com/AndPuQing/gflow/releases)
- 下载 `gflow_vX.X.X_x86_64-unknown-linux-musl.tar.gz` 压缩包
- 解压并将二进制文件复制到 `PATH` 中的目录

## 验证安装

安装后，验证 gflow 是否正确安装：

检查版本：
```bash
$ gflowd --version
<!-- cmdrun gflowd --version -->
```

```bash
$ ginfo --version
<!-- cmdrun ginfo --version -->
```

```bash
$ gbatch --version
<!-- cmdrun gbatch --version -->
```

```bash
$ gqueue --version
<!-- cmdrun gqueue --version -->
```

```bash
$ gcancel --version
<!-- cmdrun gcancel --version -->
```

验证命令是否在 PATH 中：
```bash
$ which ginfo
```

所有命令都已正确安装并在您的 PATH 中可用。

## 安装后设置

### 1. 测试 tmux
确保 tmux 正常工作：
```bash
tmux new-session -d -s test
tmux has-session -t test && echo "tmux 正常工作！"
tmux kill-session -t test
```

### 2. GPU 检测（可选）

如果您有 NVIDIA GPU，请验证它们是否被检测到：

```bash
# 启动守护进程
$ gflowd up

# 验证它已启动
$ gflowd status
```

检查系统信息和 GPU 分配：
```bash
$ ginfo
```

如果有可用的 NVIDIA GPU，守护进程会显示 GPU 信息。

### 3. 创建配置目录

gflow 会自动创建此目录，但您也可以手动创建：

```bash
mkdir -p ~/.config/gflow
mkdir -p ~/.local/share/gflow/logs
```

## 配置文件

gflow 使用以下目录：

| 位置 | 用途 |
|----------|---------|
| `~/.config/gflow/gflowd.toml` | 配置文件（可选） |
| `~/.local/share/gflow/state.json` | 持久化任务状态 |
| `~/.local/share/gflow/logs/` | 任务输出日志 |

## 故障排除

### 问题：找不到命令

如果安装后出现"找不到命令"错误：

1. **检查 `~/.cargo/bin` 是否在您的 PATH 中**：
   ```bash
   echo $PATH | grep -o ~/.cargo/bin
   ```

2. **如果缺失，添加到 PATH**（添加到 `~/.bashrc` 或 `~/.zshrc`）：
   ```bash
   export PATH="$HOME/.cargo/bin:$PATH"
   ```

3. **重新加载 shell**：
   ```bash
   source ~/.bashrc  # 或 ~/.zshrc
   ```

### 问题：未检测到 GPU

1. **检查 NVIDIA 驱动程序**：
   ```bash
   nvidia-smi
   ```

2. **验证 NVML 库**：
   ```bash
   ldconfig -p | grep libnvidia-ml
   ```

3. 如果 GPU 检测失败，gflow 仍然可以工作，但不会管理 GPU 资源。

## 更新 gflow

### 如果通过 cargo 安装：
```bash
cargo install gflow --force
```

### 如果从源代码构建：
```bash
cd gflow
git pull
cargo build --release
cargo install --path . --force
```

## 卸载

要删除 gflow：

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

现在 gflow 已安装完成，请前往[快速入门指南](./quick-start)了解如何使用它！

---

**上一页**：[介绍](/) | **下一页**：[快速入门](./quick-start)
