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

# 调度
gbatch --priority 50 python urgent.py
gbatch --name my-run python train.py

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

## 时间格式（`--time`）

- `HH:MM:SS`（例如 `2:30:00`）
- `MM:SS`（例如 `5:30`）
- `MM` 分钟（例如 `30`）

注意：单个数字表示**分钟**。30 秒请用 `0:30`。

## 内存格式（`--memory`）

- `100`（MB）
- `1024M`
- `2G`

## 脚本指令

提交脚本时，`gbatch` 可以从如下行解析少量选项：

```bash
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --time=2:00:00
# GFLOW --memory=4G
# GFLOW --priority=20
# GFLOW --conda-env=myenv
# GFLOW --depends-on=123
```

说明：

- 命令行参数优先于脚本指令。
- 脚本指令只支持 `--depends-on`（单依赖）。
