# gflowd 参考

`gflowd` 用于管理本地 gflow 守护进程。

## 用法

```bash
gflowd [options] [command]
gflowd completion <shell>
```

## 常见示例

```bash
# 交互式初始化配置
gflowd init

# 使用默认值无交互初始化
gflowd init --yes

# 启动守护进程
gflowd up

# 只使用部分 GPU，并启用随机分配策略
gflowd up --gpus 0,2 --gpu-allocation-strategy random

# 更快检测 GPU 占用变化
gflowd up --gpu-poll-interval-secs 3

# 无停机热重载
gflowd reload

# 重启并更新 GPU 限制
gflowd restart --gpus 0-3

# 查看状态或停止守护进程
gflowd status
gflowd down
```

## 全局选项

- `-c, --config <path>`：使用自定义配置文件
- `--cleanup`：清理配置文件
- `-v/-vv/-vvv/-vvvv`：提高守护进程日志级别
- `-q`：降低守护进程日志级别

## 子命令

### `gflowd init`

通过向导创建或更新配置文件。

```bash
gflowd init [--yes] [--force] [--advanced] [--gpus <indices>] [--host <host>] [--port <port>] [--timezone <tz>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
```

选项：

- `--yes`：接受所有默认值，不进入交互
- `--force`：覆盖已存在的配置文件
- `--advanced`：配置通知等高级选项
- `--gpus <indices>`：限制调度器可见的 GPU，例如 `0,2` 或 `0-2`
- `--host <host>`：守护进程地址，默认 `localhost`
- `--port <port>`：守护进程端口，默认 `59000`
- `--timezone <tz>`：写入配置的时区，例如 `Asia/Shanghai` 或 `UTC`；传 `local` 表示保持未设置
- `--gpu-allocation-strategy <strategy>`：`sequential` 或 `random`
- `--gpu-poll-interval-secs <seconds>`：每隔 N 秒轮询一次 NVML 检查 GPU 占用变化（默认 `10`，最小 `1`）

### `gflowd up`

在 tmux 会话中启动守护进程。

```bash
gflowd up [--gpus <indices>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
```

### `gflowd reload`

无停机重载守护进程。

```bash
gflowd reload [--gpus <indices>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
```

适合在不中断服务的情况下刷新正在运行的守护进程。

### `gflowd restart`

先停止，再重新启动守护进程。

```bash
gflowd restart [--gpus <indices>] [--gpu-allocation-strategy <strategy>] [--gpu-poll-interval-secs <seconds>]
```

适合可以接受完整重启的场景。

### `gflowd status`

显示守护进程是否在运行，以及健康检查是否通过。

```bash
gflowd status
```

### `gflowd down`

停止守护进程。

```bash
gflowd down
```

### `gflowd completion <shell>`

生成 shell 自动补全脚本。

```bash
gflowd completion bash
gflowd completion zsh
gflowd completion fish
```

## 说明

- `--gpus` 控制调度器为新任务分配哪些 GPU。
- `--gpu-allocation-strategy` 可选 `sequential` 或 `random`。
- `--gpu-poll-interval-secs` 控制检测非 gflow GPU 占用变化的速度。
- `up`、`reload`、`restart` 三个子命令都支持相同的 GPU 相关覆盖参数。

## 另见

- [配置](../user-guide/configuration)
- [GPU 管理](../user-guide/gpu-management)
- [快速参考](./quick-reference)
