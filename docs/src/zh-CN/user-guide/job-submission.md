# 任务提交

使用 `gbatch` 提交任务（类似 Slurm 的 `sbatch`）。你可以直接提交命令，也可以提交脚本。

::: tip
单步命令适合直接提交；如果需要环境准备、多条 shell 语句或更稳定的复用方式，优先使用脚本。
:::

## 快速开始

```bash
gbatch python train.py
gbatch --gpus 1 --time 2:00:00 --name train-resnet python train.py
gbatch --project ml-research python train.py
gbatch --notify-email alice@example.com --notify-on job_failed,job_timeout python train.py
```

## 提交命令

```bash
gbatch python train.py --epochs 100 --lr 0.01
```

如果命令包含复杂的 shell 逻辑，建议改用脚本文件。

## 提交脚本

```bash
cat > train.sh << 'EOF'
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --time=2:00:00

python train.py
EOF

chmod +x train.sh
gbatch train.sh
```

::: details 支持的脚本指令

脚本里只会解析少量选项：

- `# GFLOW --gpus=<N>`
- `# GFLOW --shared`
- `# GFLOW --time=<TIME>`
- `# GFLOW --memory=<LIMIT>`
- `# GFLOW --gpu-memory=<LIMIT>`
- `# GFLOW --priority=<N>`
- `# GFLOW --conda-env=<ENV>`
- `# GFLOW --depends-on=<job_id|@|@~N>`（仅单依赖）
- `# GFLOW --project=<CODE>`
- `# GFLOW --notify-email=<EMAIL>`
- `# GFLOW --notify-on=<EVENT1,EVENT2,...>`
:::

::: info
命令行参数优先于脚本指令。
:::

### 内存语义

- `--memory`（`--max-mem` / `--max-memory`）限制主机内存（RAM）。
- `--gpu-memory`（`--max-gpu-mem` / `--max-gpu-memory`）限制每张 GPU 的显存（VRAM）。
- 共享模式任务必须同时设置 `--shared` 和 `--gpu-memory`。

::: warning
使用 GPU 共享模式时，`--shared` 和 `--gpu-memory` 缺一不可。
:::

## 常用选项

```bash
# GPU
gbatch --gpus 1 python train.py

# 时间限制
gbatch --time 30 python quick.py

# GPU 共享模式（必须配合 --gpu-memory）
gbatch --gpus 1 --shared --gpu-memory 20G python train.py

# 优先级
gbatch --priority 50 python urgent.py

# Conda 环境
gbatch --conda-env myenv python script.py

# 项目编码
gbatch --project ml-research python train.py

# 单任务邮件通知
gbatch --notify-email alice@example.com python train.py
gbatch --notify-email alice@example.com --notify-email oncall@example.com --notify-on job_failed,job_timeout python train.py

# 依赖
gbatch --depends-on <job_id|@|@~N> python next.py
gbatch --depends-on-all 1,2,3 python merge.py
gbatch --depends-on-any 4,5 python process_first_success.py

# 语法糖：
# - @    = 最近一次提交的任务
# - @~N  = 倒数第 N+1 次提交的任务（例如 @~1 是上一次提交）

# 禁用依赖失败自动取消
gbatch --depends-on <job_id> --no-auto-cancel python next.py

# 预览但不提交
gbatch --dry-run --gpus 1 python train.py
```

::: details 依赖简写
- `@` 表示最近一次提交的任务。
- `@~N` 表示倒数第 N+1 次提交的任务。例如 `@~1` 表示上一次提交。
:::

::: info
项目值在提交后不可修改。
:::

::: info
Per-job 通知会复用 `notifications.emails` 中配置的 SMTP 发送通道。如果设置了 `--notify-email` 但没有设置 `--notify-on`，gflow 默认在终态事件时发送：`job_completed`、`job_failed`、`job_timeout`、`job_cancelled`。
:::

## 任务数组

```bash
gbatch --array 1-10 python process.py --task '$GFLOW_ARRAY_TASK_ID'
```

## 监控与日志

```bash
# 任务与分配
gqueue -f JOBID,NAME,ST,NODES,NODELIST(REASON)

# 单个任务详情（包含 GPUIDs）
gjob show <job_id>

# 日志
tail -f ~/.local/share/gflow/logs/<job_id>.log
```

## 调整或重提

- 修改排队/暂停任务：`gjob update <job_id> ...`
- 重新提交任务：`gjob redo <job_id>`（用 `--cascade` 级联重做依赖任务）

## 另见

- [任务依赖](./job-dependencies) - 工作流与依赖模式
- [时间限制](./time-limits) - 时间格式与行为
- [GPU 管理](./gpu-management) - 分配细节
