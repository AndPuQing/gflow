# 配置

大多数情况下 gflow 无需配置即可使用。只有在你需要调整守护进程监听地址/端口，或限制可用 GPU 时才需要配置文件（TOML）或环境变量。

## 配置文件

默认位置：

```
~/.config/gflow/gflow.toml
```

最小示例：

```toml
[daemon]
host = "localhost"
port = 59000
# gpus = [0, 2]
```

所有命令都支持 `--config <path>` 指定配置文件：

```bash
gflowd --config <path> up
ginfo --config <path>
gbatch --config <path> --gpus 1 python train.py
```

## 守护进程配置

### 主机和端口

```toml
[daemon]
host = "localhost"
port = 59000
```

- 默认：`localhost:59000`
- 仅在明确了解安全影响时使用 `0.0.0.0`。

<a id="gpu-selection"></a>

#### GPU 选择

限制调度器允许分配的物理 GPU。

配置文件：

```toml
[daemon]
gpus = [0, 2]
```

守护进程 CLI 参数（覆盖配置文件）：

```bash
gflowd up --gpus 0,2
gflowd restart --gpus 0-3
```

运行时控制（只影响新的分配）：

```bash
gctl set-gpus 0,2
gctl set-gpus all
gctl show-gpus
```

支持写法：`0`、`0,2,4`、`0-3`、`0-1,3,5-6`。

优先级（从高到低）：
1. CLI 参数（`gflowd up --gpus ...`）
2. 环境变量（`GFLOW_DAEMON_GPUS=...`）
3. 配置文件（`daemon.gpus = [...]`）
4. 默认：所有检测到的 GPU

### 日志

- `gflowd`：使用 `-v/--verbose`（见 `gflowd --help`）。
- 客户端命令（`gbatch`、`gqueue`、`ginfo`、`gjob`、`gctl`）：使用 `RUST_LOG`（例如 `RUST_LOG=info`）。

## 环境变量

```bash
export GFLOW_DAEMON_HOST=localhost
export GFLOW_DAEMON_PORT=59000
export GFLOW_DAEMON_GPUS=0,2
```

## 文件与状态

gflow 遵循 XDG Base Directory 规范：

```text
~/.config/gflow/gflow.toml
~/.local/share/gflow/state.json
~/.local/share/gflow/logs/<job_id>.log
```

## 故障排除

### 找不到配置文件

```bash
ls -la ~/.config/gflow/gflow.toml
```

### 端口被占用

更换端口：

```toml
[daemon]
port = 59001
```

## 另见

- [安装](../getting-started/installation) - 初始设置
- [快速开始](../getting-started/quick-start) - 基本用法
- [GPU 管理](./gpu-management) - GPU 分配
