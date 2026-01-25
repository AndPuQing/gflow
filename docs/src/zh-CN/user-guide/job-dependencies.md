# 任务依赖

任务依赖用于构建工作流：一个任务会等待其他任务完成后才开始运行。

## 快速开始

```bash
gbatch --time 10 python preprocess.py
gbatch --depends-on @ --gpus 1 --time 4:00:00 python train.py
gbatch --depends-on @ --time 10 python evaluate.py
```

## 依赖方式

### 单依赖

```bash
gbatch --depends-on <job_id|@|@~N> python next.py
```

语法糖：
- `@`：最近一次提交的任务
- `@~N`：倒数第 N+1 次提交的任务（例如 `@~1` 是上一次提交）

### 多依赖（AND / OR）

```bash
# AND：所有父任务都必须成功完成
gbatch --depends-on-all 101,102,103 python merge.py

# OR：任意一个父任务成功完成即可继续
gbatch --depends-on-any 201,202,203 python process_first_success.py
```

`@` 语法同样可用于列表（例如 `--depends-on-all @,@~1,@~2`）。

### 脚本指令

脚本指令只支持 `--depends-on`（单依赖）：

```bash
#!/bin/bash
# GFLOW --depends-on=123

python next.py
```

## 自动取消

默认情况下，当依赖任务失败/取消/超时，依赖它的任务会被自动取消。若希望它们继续保持排队状态，可禁用自动取消：

```bash
gbatch --depends-on <job_id> --no-auto-cancel python next.py
```

禁用后，即使父任务失败，依赖任务也不会自动继续运行；你需要手动取消或重新提交。

## 监控依赖

```bash
# 树视图
gqueue -t
```

示例输出：
```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

```bash
# 只看部分任务
gqueue -j <job_id>,<job_id> -t

# 查看排队原因（通常是等待依赖或资源）
gqueue -s Queued -f JOBID,NAME,ST,NODELIST(REASON)
```

## 故障排除

### 依赖任务未启动

```bash
gqueue -t
gqueue -j <parent_job_id> -f JOBID,ST
ginfo
```

### 修复失败后重做整条链

```bash
gjob redo <job_id> --cascade
```

## 另见

- [任务生命周期](./job-lifecycle) - 原因与状态流转
- [时间限制](./time-limits) - 防止任务无限运行
