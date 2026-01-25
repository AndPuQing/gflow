# 任务提交

使用 `gbatch` 提交任务（类似 Slurm 的 `sbatch`）。你可以直接提交命令，也可以提交脚本。

## 快速开始

```bash
gbatch python train.py
gbatch --gpus 1 --time 2:00:00 --name train-resnet python train.py
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

### 脚本指令

脚本里只会解析少量选项：

- `# GFLOW --gpus=<N>`
- `# GFLOW --time=<TIME>`
- `# GFLOW --memory=<LIMIT>`
- `# GFLOW --priority=<N>`
- `# GFLOW --conda-env=<ENV>`
- `# GFLOW --depends-on=<job_id|@|@~N>`（仅单依赖）

命令行参数优先于脚本指令。

## 常用选项

```bash
# GPU
gbatch --gpus 1 python train.py

# 时间限制
gbatch --time 30 python quick.py

# 优先级
gbatch --priority 50 python urgent.py

# Conda 环境
gbatch --conda-env myenv python script.py

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
