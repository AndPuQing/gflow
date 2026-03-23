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
gbatch --notify-email alice@example.com --notify-on job_failed,job_timeout python train.py

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

`--gpu-memory` 控制每张 GPU 的显存（VRAM）。

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

## 单任务通知（`--notify-email`、`--notify-on`）

- 使用 `--notify-email <address>` 可重复添加该任务的邮件收件人。
- 使用 `--notify-on <event1,event2,...>` 选择触发这些邮件的事件。
- 如果设置了 `--notify-email` 但没有设置 `--notify-on`，gflow 默认在 `job_completed`、`job_failed`、`job_timeout`、`job_cancelled` 时发送。
- 收件人会合并脚本指令与命令行；如果命令行提供了 `--notify-on`，则覆盖脚本中的事件列表。
- 实际发送仍然复用 `notifications.emails` 下配置的全局 SMTP 通道。
