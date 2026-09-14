# gbatch 参考

`gbatch` 用于提交任务到调度器（类似 Slurm `sbatch`）。

## 用法

```bash
gbatch [options] <script>
gbatch [options] <command> [args...]
gbatch new <name>
gbatch completion <shell>
```

## 常用选项

```bash
# 资源
gbatch --gpus 1 python train.py
gbatch --time 2:00:00 python train.py
gbatch --memory 8G python train.py
gbatch --gpu-memory 20G --shared --gpus 1 python train.py

# 调度
gbatch --priority 50 python urgent.py
gbatch --name my-run python train.py
gbatch --project ml-research python train.py
gbatch --max-retries 2 python train.py
gbatch --notify-email alice@example.com --notify-on job_failed,job_timeout python train.py

# 定时 / 延时启动：任务保持 Queued（原因 "BeginTime"），到点后自动进入调度
gbatch --begin 22:00 python night-run.py
gbatch --begin 2026-02-01T08:00:00 python monthly-report.py
gbatch --begin now+2h python train.py        # 相对时间：默认分钟，支持 s/m/h/d 单位

# 环境
gbatch --conda-env myenv python script.py

# 依赖
gbatch --depends-on <job_id|@|@~N> python next.py
gbatch --depends-on-all 1,2,3 python merge.py     # AND
gbatch --depends-on-any 4,5 python fallback.py    # OR
gbatch --depends-on 123 --no-auto-cancel python next.py

语法糖：`@` = 最近一次提交的任务，`@~N` = 倒数第 N+1 次提交的任务。

# 数组
gbatch --array 1-10 python task.py --i '$GFLOW_ARRAY_TASK_ID'

# 参数（笛卡尔积展开）
gbatch --param lr=0.001,0.01 --param bs=32,64 python train.py --lr {lr} --batch-size {bs}
gbatch --param-file params.csv --name-template 'run_{id}' python train.py --id {id}
gbatch --max-concurrent 2 --param lr=0.001,0.01 python train.py --lr {lr}

# 预览
gbatch --dry-run --gpus 1 python train.py
```

## Slurm 兼容别名

为降低从 Slurm `sbatch` 迁移成本，`gbatch` 支持部分常用参数别名：

- `--nice` → `--priority`
- `--job-name`（或 `-J`）→ `--name`
- `--gres` → `--gpus`（需要整数 GPU 数量，例如 `--gres 2`）
- `--dependency` → `--depends-on`
- `--time-limit` / `--timelimit` → `--time`

## 时间格式（`--time`）

- `HH:MM:SS`（例如 `2:30:00`）
- `MM:SS`（例如 `5:30`）
- `MM` 分钟（例如 `30`）

注意：单个数字表示**分钟**。30 秒请用 `0:30`。

## 内存格式（`--memory`）

- `100`（MB）
- `1024M`
- `2G`

别名：`--max-mem`、`--max-memory`。

`--memory` 控制主机内存（RAM），不是 GPU 显存。

## GPU 显存格式（`--gpu-memory`）

- `8192`（MB）
- `16384M`
- `24G`

别名：`--max-gpu-mem`、`--max-gpu-memory`。

`--gpu-memory` 控制每张 GPU 的显存（VRAM），在两种共享模式下都会生效：

- **共享任务**必须指定。它限制每个任务在单卡上的公平份额，调度器只有在
  `已分配 + 本任务需求` 仍能装下时才接纳该任务。
- **独占任务**也可以指定。此时它作为硬性校验，对比的是设备**实际**可用显存，
  包含非 gflow 进程（例如你手动启动的 vLLM 服务）占用的部分。若需求在任何一张卡上
  都装不下，任务会保持 `Queued` 并带 `GpuMemory: ...` 原因，而不是先启动、
  训练一段时间后才 CUDA OOM 失败。

独占任务不指定 `--gpu-memory` 表示「未声明需求」：gflow 不会干预任务自己管理显存的方式。

提交时 `gbatch` 还会打印当前各卡显存占用，例如：

```text
Current GPU memory usage:
  GPU 0: 665 MiB used / 94.97 GiB total (94.32 GiB free)
  GPU 1: 54.7 GiB used / 94.97 GiB total (40.27 GiB free)
```

## GPU 共享模式（`--shared`）

- `--shared` 允许任务与其他共享任务共用同一张 GPU。
- 共享任务必须同时指定 `--gpu-memory`。
- `--shared` 不会与独占任务在同一张 GPU 上混跑。

## 脚本指令

提交脚本时，`gbatch` 可以从如下行解析少量选项：

```bash
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --shared
# GFLOW --time=2:00:00
# GFLOW --memory=4G
# GFLOW --gpu-memory=20G
# GFLOW --priority=20
# GFLOW --conda-env=myenv
# GFLOW --depends-on=123
# GFLOW --project=ml-research
# GFLOW --notify-email=alice@example.com
# GFLOW --notify-on=job_failed,job_timeout
```

说明：

- 命令行参数优先于脚本指令。
- 脚本指令只支持 `--depends-on`（单依赖）。

## 项目标记（`--project`）

- 使用 `-P/--project <code>` 为任务附加可选项目编码。
- 项目值会自动去除首尾空白；空白字符串会被视为未设置。
- 最大长度为 64 个字符。
- 项目值在提交后不可修改。
- 命令行 `--project` 会覆盖脚本中的 `# GFLOW --project=...`。

## 自动重试（`--max-retries`）

- 使用 `--max-retries <N>`，允许任务在执行失败后最多自动重提 `N` 次。
- 当前只有任务从 `Running` 以非零退出时才会触发自动重试。
- 超时暂时不会自动重试。
- 如果失败任务还有排队中的下游依赖，gflow 会自动把它们改挂到最新一次重试任务上。

## 单任务通知（`--notify-email`、`--notify-on`）

- 使用 `--notify-email <address>` 可重复添加该任务的邮件收件人。
- 使用 `--notify-on <event1,event2,...>` 选择触发这些邮件的事件。
- 如果设置了 `--notify-email` 但没有设置 `--notify-on`，gflow 默认在 `job_completed`、`job_failed`、`job_timeout`、`job_cancelled` 时发送。
- 收件人会合并脚本指令与命令行；如果命令行提供了 `--notify-on`，则覆盖脚本中的事件列表。
- 实际发送仍然复用[通知](../user-guide/notifications)里配置的全局 SMTP 通道。
