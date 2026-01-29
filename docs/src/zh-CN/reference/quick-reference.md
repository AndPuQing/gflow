# gflow 快速参考

## 守护进程

```bash
gflowd up
gflowd status
gflowd down
gflowd restart
```

## 查看与监控

```bash
# GPU（可用性 + 分配情况）
ginfo

# 任务（默认：最近 10 个）
gqueue
gqueue -a

# 常用格式
gqueue -f JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
gqueue -s Running -f JOBID,NAME,ST,NODES,NODELIST(REASON)

# 依赖树
gqueue -t
```

`gqueue -t` 示例输出：
```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

## 提交任务（`gbatch`）

```bash
# 提交命令
gbatch python train.py --epochs 100

# 提交脚本
gbatch train.sh

# 常用选项
gbatch --gpus 1 --time 2:00:00 --name train-resnet python train.py
gbatch --priority 50 python urgent.py
gbatch --conda-env myenv python script.py
gbatch --dry-run --gpus 1 python train.py
```

### 脚本指令

脚本里只会解析这些指令：

```bash
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --time=2:00:00
# GFLOW --memory=4G
# GFLOW --priority=20
# GFLOW --conda-env=myenv
# GFLOW --depends-on=123
```

命令行参数优先于脚本指令。

## 任务依赖

```bash
# 单依赖
gbatch --depends-on <job_id|@|@~N> python next.py

# 多依赖
gbatch --depends-on-all 1,2,3 python merge.py     # AND
gbatch --depends-on-any 4,5 python fallback.py    # OR

# 语法糖：@ = 最近一次提交，@~N = 倒数第 N+1 次提交

# 依赖失败行为
gbatch --depends-on 123 --no-auto-cancel python next.py
```

## 任务数组

```bash
gbatch --array 1-10 python process.py --task '$GFLOW_ARRAY_TASK_ID'
```

## 参数（`--param`）

```bash
gbatch --param lr=0.001,0.01 --param bs=32,64 python train.py --lr {lr} --batch-size {bs}
gbatch --param-file params.csv --name-template 'run_{id}' python train.py --id {id}
```

## 控制命令

```bash
# 取消（用 --dry-run 查看依赖任务）
gcancel <job_id>
gcancel --dry-run <job_id>

# 暂停/恢复
gjob hold <job_id>
gjob release <job_id>

# 详情 / 重做 / 更新
gjob show <job_id>
gjob redo <job_id>
gjob redo <job_id> --cascade
gjob update <job_id> --gpus 2 --time-limit 4:00:00
```

## 运行时控制（`gctl`）

```bash
# 限制调度器允许分配的 GPU（只影响新的分配）
gctl show-gpus
gctl set-gpus 0,2
gctl set-gpus all

# 任务组并发限制
gctl set-limit <job_or_group_id> 2

# GPU 预留（按用户/时间窗口预留 GPU）
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
gctl reserve list --active
gctl reserve list --timeline --range 48h
gctl reserve cancel <reservation_id>
```

## 时间格式（`--time`）

- `HH:MM:SS`（例如 `2:30:00`）
- `MM:SS`（例如 `5:30`）
- `MM` 分钟（例如 `30`）

注意：单个数字表示**分钟**。30 秒请用 `0:30`。

## 状态码

| 代码 | 状态 |
|------|------|
| `PD` | Queued |
| `H`  | Hold |
| `R`  | Running |
| `CD` | Finished |
| `F`  | Failed |
| `CA` | Cancelled |
| `TO` | Timeout |

## 路径

```text
~/.config/gflow/gflow.toml
~/.local/share/gflow/state.json
~/.local/share/gflow/logs/<job_id>.log
```

## 另见

- [任务提交](../user-guide/job-submission)
- [任务依赖](../user-guide/job-dependencies)
- [时间限制](../user-guide/time-limits)
- [GPU 管理](../user-guide/gpu-management)
