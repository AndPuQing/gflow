# 配置

本指南涵盖了如何为您的环境配置 gflow。

## 概述

gflow 使用基于 TOML 文件和环境变量的简单配置系统。大多数用户可以在没有任何配置的情况下使用 gflow，但对于特定需求可以使用自定义选项。

## 配置文件

### 默认配置位置

```
~/.config/gflow/gflow.toml
```

当您首次运行 gflow 命令时，此文件会自动创建。如果不存在，gflow 使用内置默认值。

### 配置文件结构

```toml
[daemon]
# 守护进程连接设置
host = "localhost"
port = 59000

# 可选：指定要使用的 GPU 索引（注释掉 = 使用全部）
# gpus = [0, 1, 2]

# 可选：日志级别（error、warn、info、debug、trace）
# log_level = "info"
```

### 自定义配置位置

使用 `--config` 标志（在所有命令上可用，但在帮助中隐藏）：

```bash
# 使用自定义配置文件
gflowd --config /path/to/custom.toml up
gflowd --config /path/to/custom.toml status
ginfo --config /path/to/custom.toml info
gflowd --config /path/to/custom.toml down
gbatch --config /path/to/custom.toml ...
gqueue --config /path/to/custom.toml
```

## 配置选项

### 守护进程配置

#### 主机和端口

控制守护进程监听的位置：

```toml
[daemon]
host = "localhost"  # 监听地址
port = 59000        # 监听端口
```

**默认值**：
- 主机：`localhost`（127.0.0.1）
- 端口：`59000`

**用途**：
- 默认值适合单机使用
- 如果 59000 已在使用，更改端口
- 使用 `0.0.0.0` 允许远程连接（⚠️ 不建议用于安全性）

#### GPU 选择

通过配置文件、CLI 标志或运行时命令限制 gflow 可以使用的 GPU。

**配置文件**（`~/.config/gflow/gflow.toml`）：
```toml
[daemon]
# 仅使用 GPU 0 和 2
gpus = [0, 2]
```

**CLI 标志**（覆盖配置文件）：
```bash
# 使用 GPU 限制启动守护进程
gflowd up --gpus 0,2

# 使用不同的 GPU 重新启动
gflowd restart --gpus 0-3
```

**运行时控制**（在守护进程运行时更改 GPU）：
```bash
# 限制为特定 GPU
gctl set-gpus 0,2

# 使用 GPU 范围
gctl set-gpus 0-3

# 允许所有 GPU
gctl set-gpus all

# 检查当前配置
gctl show-gpus
```

**支持的语法**：
- 单个 GPU：`0`
- 逗号分隔：`0,2,4`
- 范围：`0-3`（展开为 0,1,2,3）
- 混合：`0-1,3,5-6`

**工作原理**：
- 调度器仅将任务分配给允许的 GPU
- 无效的 GPU 索引被记录为警告并忽略
- 运行中的任务在限制更改时保持不变
- 限制在守护进程重新启动后持续
- CLI 标志覆盖配置文件设置

**用途**：
- 为其他应用程序保留特定 GPU
- 使用 GPU 子集进行测试
- 将 gflow 与其他工作负载隔离
- 在不重新启动的情况下动态调整 GPU 可用性

**示例**：

查看当前 GPU 配置：
```bash
$ gctl show-gpus
=== GPU Configuration ===

GPU Restriction: Only GPUs [0, 2] are allowed

=== Detected GPUs ===

GPU 0: Available
GPU 1: In Use (RESTRICTED)
GPU 2: Available
GPU 3: Available (RESTRICTED)
```

在运行时更改限制：
```bash
# 当前使用 GPU 0,2
$ gctl show-gpus
GPU Restriction: Only GPUs [0, 2] are allowed

# 更改为仅使用 GPU 0
$ gctl set-gpus 0
GPU restriction updated: only GPUs [0] will be used

# 任务现在只能使用 GPU 0
# GPU 2 上的运行中任务继续，但新任务不会使用它
```

优先级顺序（从高到低）：
1. CLI 标志：`gflowd up --gpus 0,2`
2. 环境变量：`GFLOW_DAEMON__GPUS='[0,2]'`
3. 配置文件：`gpus = [0, 2]`
4. 默认值：所有检测到的 GPU

**默认值**：所有检测到的 GPU 都可用

#### 日志级别

控制守护进程的详细程度：

```toml
[daemon]
log_level = "info"  # error | warn | info | debug | trace
```

**级别**：
- `error`：仅关键错误
- `warn`：警告和错误
- `info`：一般信息（默认）
- `debug`：详细调试信息
- `trace`：非常详细（包括所有内部操作）

## 环境变量

### 通过环境配置

gflow 支持带 `GFLOW_` 前缀的环境变量配置：

```bash
# 设置守护进程主机
export GFLOW_DAEMON_HOST="localhost"

# 设置守护进程端口
export GFLOW_DAEMON_PORT="59000"

# 设置日志级别
export GFLOW_LOG_LEVEL="debug"

# 使用这些设置启动守护进程
gflowd up
```

**优先级**：
1. 命令行参数（如果可用）
2. 配置文件（`--config` 或默认）
3. 环境变量
4. 内置默认值

## 文件位置

### 标准目录

gflow 使用 XDG 基本目录规范：

```bash
# 配置
~/.config/gflow/
  └── gflow.toml          # 主配置文件

# 数据（状态和日志）
~/.local/share/gflow/
  ├── state.json           # 持久任务状态
  └── logs/                # 任务输出日志
      ├── 1.log
      ├── 2.log
      └── ...

# 运行时（可选，默认不使用）
~/.local/share/gflow/
```

## 故障排除配置

### 问题：找不到配置文件

**检查位置**：
```bash
ls -la ~/.config/gflow/gflow.toml
```

**解决方案**：创建默认配置或使用 `--config` 指定

### 问题：端口已在使用

**检查端口**：
```bash
lsof -i :59000
```

**解决方案**：
1. 在配置中更改端口：
   ```toml
   [daemon]
   port = 59001
   ```

2. 杀死使用该端口的进程：
   ```bash
   kill <PID>
   ```

## 最佳实践

1. **使用默认配置** 除非您有特定需求
2. **定期备份状态** 如果任务历史很重要
3. **定期清理日志** 以管理磁盘空间


## 另见

- [安装](../getting-started/installation) - 初始设置
- [快速开始](../getting-started/quick-start) - 基本用法
- [任务提交](./job-submission) - 提交任务
- [GPU 管理](./gpu-management) - GPU 分配
