# gflow 快速参考卡

## 基本命令

### 守护进程管理
```bash
gflowd up                  # 启动调度器守护进程
gflowd down                # 停止调度器守护进程
ginfo              # 检查守护进程状态和 GPU 分配
watch -n 2 ginfo   # 监控调度器状态
```

### 任务提交
```bash
# 基本提交
gbatch python script.py
gbatch my_script.sh

# 使用 GPU
gbatch --gpus 1 python train.py

# 设置时间限制
gbatch --time 2:00:00 python train.py    # 2 小时
gbatch --time 30 python train.py         # 30 分钟
gbatch --time 5:30 python train.py       # 5 分 30 秒

# 设置依赖关系
gbatch --depends-on 123 python process.py

# 设置优先级
gbatch --priority 100 python urgent.py

# 任务数组
gbatch --array 1-10 python task.py

# Conda 环境
gbatch --conda-env myenv python script.py

# 组合选项
gbatch --gpus 2 --time 4:00:00 --priority 50 \
       python train.py
```

### 任务脚本格式
```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 2:00:00
# GFLOW --priority 20
# GFLOW --conda-env myenv

echo "Starting job..."
python train.py
```

### 查询任务
```bash
# 基本列表
gqueue                           # 显示最后 10 个任务
gqueue -a                        # 显示所有任务
gqueue -n 20                     # 显示最后 20 个任务

# 按状态筛选
gqueue -s Running                # 仅运行中的任务
gqueue -s Queued,Running         # 多个状态

# 按任务 ID 筛选
gqueue -j 42                     # 特定任务
gqueue -j 40,41,42               # 多个任务（逗号分隔）
gqueue -j 40-45                  # 任务 ID 范围（40, 41, 42, 43, 44, 45）

# 自定义格式
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
gqueue -f JOBID,NAME,ST,NODES,NODELIST

# 排序选项
gqueue -r id                     # 按 ID 排序（默认）
gqueue -r time                   # 按开始时间排序
gqueue -r priority               # 按优先级排序

# 按状态分组
gqueue -g                        # 按状态分组任务

# 依赖关系树
gqueue -t                        # 显示任务依赖关系树
```

### 任务控制
```bash
# 取消任务
gcancel <job_id>                 # 取消任务
gcancel 40,41,42                 # 取消多个任务（逗号分隔）
gcancel 40-45                    # 取消任务 ID 范围（40 到 45）
gcancel --dry-run <job_id>       # 预览取消影响（显示依赖任务）
gcancel --finish <job_id>        # 标记任务为已完成（内部使用）
gcancel --fail <job_id>          # 标记任务为失败（内部使用）

# 暂停和释放任务
gjob hold <job_id>               # 暂停排队任务
gjob release <job_id>            # 将暂停的任务释放回队列

# 重做任务
gjob redo <job_id>               # 使用相同参数重新提交任务
gjob redo <job_id> --gpus 4      # 使用修改的 GPU 数量重新提交
gjob redo <job_id> --time 8:00:00 # 使用更长的时间限制重新提交
gjob redo <job_id> --cascade     # 重做任务及所有依赖任务

# 更新排队任务
gjob update <job_id> --gpus 2                    # 更新 GPU 数量
gjob update <job_id> --priority 15               # 更新优先级
gjob update <job_id> --time-limit 04:00:00       # 更新时间限制
gjob update <job_id> --depends-on 100,101        # 更新依赖关系
gjob update <job_id> --param batch_size=64       # 更新参数
gjob update <job_id> --gpus 4 --priority 20      # 更新多个字段
```

### 监控
```bash
# 监控队列
watch -n 5 gqueue

# 监控运行中的任务及时间限制
watch -n 5 'gqueue -s Running -f JOBID,NAME,TIME,TIMELIMIT'

# 查看日志
cat ~/.local/share/gflow/logs/<job_id>.log
tail -f ~/.local/share/gflow/logs/<job_id>.log

# 连接到守护进程 tmux 会话
tmux attach -t gflow_server

# 连接到任务 tmux 会话
tmux attach -t <job_run_name>
```

## 任务状态

| 代码 | 完整名称 | 描述 |
|------|---------|------|
| `PD` | Queued | 等待资源或依赖 |
| `R` | Running | 正在执行 |
| `CD` | Finished | 成功完成 |
| `F` | Failed | 以非零状态退出 |
| `CA` | Cancelled | 被用户手动取消 |
| `TO` | Timeout | 超过时间限制 |

## 时间限制格式

| 格式 | 示例 | 含义 |
|------|------|------|
| `HH:MM:SS` | `2:30:00` | 2 小时 30 分钟 |
| `HH:MM:SS` | `12:00:00` | 12 小时 |
| `MM:SS` | `45:30` | 45 分钟 30 秒 |
| `MM:SS` | `5:00` | 5 分钟 |
| `MM` | `30` | 30 分钟 |
| `MM` | `120` | 120 分钟（2 小时） |

**注意**：单个数字始终表示分钟，不是秒！

## 输出格式字段

`gqueue -f` 可用的字段：
- `JOBID` - 任务 ID 号
- `NAME` - 任务运行名称（tmux 会话名称）
- `ST` - 状态（短形式）
- `TIME` - 已用时间（HH:MM:SS）
- `TIMELIMIT` - 时间限制（HH:MM:SS 或 UNLIMITED）
- `NODES` - 请求的 GPU 数量
- `NODELIST(REASON)` - 分配的 GPU ID

## 环境变量

gflow 在任务环境中设置的变量：
- `CUDA_VISIBLE_DEVICES` - 逗号分隔的 GPU ID
- `GFLOW_ARRAY_TASK_ID` - 数组任务 ID（非数组任务为 0）

## 文件位置

```
~/.config/gflow/
  └── gflowd.toml              # 配置文件

~/.local/share/gflow/
  ├── state.json               # 任务状态（持久化）
  └── logs/
      ├── 1.log                # 任务输出日志
      ├── 2.log
      └── ...
```

## 常见模式

### 顺序任务（管道）
```bash
# 步骤 1：预处理
gbatch --time 30 python preprocess.py

# 步骤 2：训练（依赖步骤 1）
gbatch --time 4:00:00 --depends-on @ python train.py

# 步骤 3：评估（依赖步骤 2）
gbatch --time 10 --depends-on @ python evaluate.py
```

`@` 符号引用最近提交的任务，使管道变得简单。

### 并行任务（数组）
```bash
# 并行处理 10 个任务
gbatch --array 1-10 --time 1:00:00 \
       python process.py --task $GFLOW_ARRAY_TASK_ID
```

### GPU 扫描
```bash
# 在不同 GPU 上尝试不同的超参数
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.001
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.01
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.1
```

### 长时间运行且带检查点
```bash
# 初始训练
gbatch --time 8:00:00 --gpus 1 \
       python train.py --checkpoint checkpoint.pth

# 如果超时则恢复（检查后手动提交）
gbatch --time 8:00:00 --gpus 1 \
       python train.py --resume checkpoint.pth
```

## 技巧和窍门

### 1. 依赖完成时自动提交
```bash
# 没有内置支持，但可以编写脚本：
while [ "$(gqueue -j $ID1 -f ST)" != "CD" ]; do sleep 5; done
gbatch python next_step.py
```

### 2. 以编程方式获取任务输出路径
```bash
JOB_ID=42
LOG_PATH="$HOME/.local/share/gflow/logs/${JOB_ID}.log"
tail -f "$LOG_PATH"
```

### 3. 检查剩余时间（手动）
```bash
# 显示时间和限制
gqueue -j 42 -f TIME,TIMELIMIT

# 示例输出：
# TIME         TIMELIMIT
# 01:23:45     02:00:00
# 剩余时间：约 36 分钟
```

### 4. 筛选超时任务
```bash
gqueue -s Timeout -f JOBID,NAME,TIME,TIMELIMIT
```

### 5. 快速任务状态检查
```bash
# 检查任务是否成功完成
[ "$(gqueue -j 42 -f ST)" = "CD" ] && echo "Success!" || echo "Not done or failed"
```

### 6. 取消所有运行中的任务
```bash
# 获取所有运行中的任务 ID
RUNNING=$(gqueue -s Running -f JOBID | tail -n +2)
for jobid in $RUNNING; do
    gcancel $jobid
done
```

### 7. 查找超时的任务
```bash
gqueue -a -s Timeout -f JOBID,NAME,TIME,TIMELIMIT
```

## 故障排除

### 任务卡在队列中
```bash
# 检查依赖关系
gqueue -t

# 检查 GPU 可用性
gqueue -s Running -f JOBID,NODES,NODELIST

# 检查依赖任务是否完成
gqueue -j <dependency_id> -f ST
```

### 任务意外超时
```bash
# 检查实际运行时间
gqueue -j <job_id> -f TIME,TIMELIMIT

# 验证时间格式（30 = 30 分钟，不是秒！）
# 使用更长的限制重新提交
gbatch --time 60 ...
```

### 找不到任务日志
```bash
# 日志位置
ls -la ~/.local/share/gflow/logs/

# 检查任务 ID 是否正确
gqueue -a -f JOBID,NAME
```

### 任务未获得 GPU
```bash
# 检查是否请求了 GPU
gqueue -j <job_id> -f JOBID,NODES,NODELIST

# 检查 GPU 可用性
nvidia-smi

# 检查其他任务是否在使用 GPU
gqueue -s Running -f JOBID,NODES,NODELIST
```

## 资源限制

默认调度器设置：
- **检查间隔**：5 秒
- **超时精度**：±5 秒
- **时间限制范围**：无硬限制
- **优先级范围**：0-255（默认：10）
- **GPU 检测**：通过 NVML（仅 NVIDIA GPU）

## 退出代码

日志中的常见退出代码：
- `0` - 成功
- `1` - 一般错误
- `130` - SIGINT（Ctrl-C / 超时）
- `137` - SIGKILL（强制终止）
- `143` - SIGTERM（优雅终止）

## 快速诊断

| 症状 | 可能原因 | 解决方案 |
|------|---------|---------|
| 任务显示 TO | 超过时间限制 | 增加 `--time` |
| 任务显示 F | 脚本错误 | 检查日志 |
| 任务卡在 PD | 依赖未完成 | 检查依赖状态 |
| 任务卡在 PD | 没有可用 GPU | 等待或减少 `--gpus` |
| 无输出 | Pipe-pane 问题 | 检查 tmux 会话 |
| 无法连接 | 会话已关闭 | 任务可能已完成 |

## 最佳实践

1. **始终为生产任务设置时间限制**
2. **对并行独立任务使用任务数组**
3. **为长时间运行的任务实现检查点**
4. **使用 `watch gqueue` 监控时间使用情况**
5. **为时间估计添加缓冲**（10-20%）
6. **对管道工作流使用依赖关系**
7. **任务失败或超时时检查日志**
8. **提交前使用短时间限制测试脚本**

## 获取帮助

- 详细文档：`docs/TIME_LIMITS.md`
- 主要 README：`README.md`
- 报告问题：GitHub Issues
- 源代码：GitHub Repository

---

**快速帮助**：使用 `--help` 运行任何命令以获取详细选项：
```bash
$ gbatch --help
<!-- cmdrun gbatch --help -->
```

```bash
$ gqueue --help
<!-- cmdrun gqueue --help -->
```

```bash
$ ginfo --help
<!-- cmdrun ginfo --help -->
```
