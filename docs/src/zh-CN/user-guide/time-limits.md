# 时间限制

为任务设置时间限制可以防止任务无限运行。当运行中的任务超过限制时，gflow 会终止它并标记为 `Timeout`（`TO`）。

## 设置时间限制

使用 `gbatch`：

```bash
gbatch --time <TIME> python train.py
```

在脚本中（指令）：

```bash
#!/bin/bash
# GFLOW --time 2:00:00

python train.py
```

命令行参数优先于脚本指令。

## 时间格式

`<TIME>` 支持：

- `HH:MM:SS`（例如 `2:30:00`）
- `MM:SS`（例如 `5:30`）
- `MM` 分钟（例如 `30`）

注意：单个数字表示**分钟**（所以 `--time 30` 是 30 分钟，不是 30 秒）。30 秒请用 `0:30`。

## 查看时间限制

```bash
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
gjob show <job_id>
```

## 行为说明

- 计时从任务进入 `Running` 开始（排队时间不计入）。
- 以周期方式检查（可能会略微超过精确限制）。
- 超时后会发送中断（Ctrl-C / SIGINT），并将状态切换为 `Timeout`（`TO`）。

## 故障排除

### 任务超时

增大限制并重新提交：

```bash
gjob redo <job_id> --time 4:00:00
```

### 任务比预期更早结束

检查格式（分钟 vs 秒）：

```bash
gbatch --time 0:30 sleep 1000   # 30 秒
gbatch --time 30 sleep 1000     # 30 分钟
```

### 没有按时间限制终止

```bash
ginfo
gqueue -j <job_id> -f JOBID,ST,TIMELIMIT
```

## 另见

- [任务生命周期](./job-lifecycle) - 任务状态（包含 `TO`）
- [任务提交](./job-submission) - 提交选项
- [快速参考](../reference/quick-reference) - 命令速查表
