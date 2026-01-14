# gflow 中的任务时间限制

## 概述

gflow 支持为任务设置时间限制，类似于 Slurm 的 `sbatch --time` 参数。当任务超过其指定的时间限制时，调度器会自动终止它并将其标记为 `Timeout` 状态。此功能有助于防止失控任务无限期消耗资源，并便于更好的资源规划。

## 目录

- [基本用法](#基本用法)
- [时间格式规范](#时间格式规范)
- [行为和执行](#行为和执行)
- [任务状态](#任务状态)
- [示例](#示例)
- [最佳实践](#最佳实践)
- [常见问题](#常见问题)

## 基本用法

### 使用 `gbatch` 设置时间限制

提交任务时使用 `--time`（或 `-t`）标志：

```bash
gbatch --time <TIME_SPEC> your_command
```

### 在任务脚本中

您也可以使用 `# GFLOW` 指令直接在任务脚本中指定时间限制：

```bash
#!/bin/bash
# GFLOW --time 2:00:00
# GFLOW --gpus 1

echo "Starting training..."
python train.py
```

注意：命令行参数优先于脚本指令。

## 时间格式规范

gflow 支持多种时间格式规范以提高灵活性：

| 格式 | 描述 | 示例 | 等效时长 |
|------|------|------|---------|
| `HH:MM:SS` | 小时:分钟:秒 | `2:30:45` | 2 小时、30 分钟、45 秒 |
| `MM:SS` | 分钟:秒 | `45:30` | 45 分钟、30 秒 |
| `MM` | 仅分钟 | `30` | 30 分钟 |

### 格式详情

#### `HH:MM:SS` 格式
- **用途**：长时间运行的任务（小时）
- **示例**：`--time 1:30:00`（1.5 小时）
- **范围**：小时、分钟和秒的任何有效组合

#### `MM:SS` 格式
- **用途**：中等时长任务（分钟）
- **示例**：`--time 45:30`（45 分钟、30 秒）
- **注意**：用冒号分隔的两个数字被解释为分钟:秒

#### `MM` 格式
- **用途**：短任务（分钟）
- **示例**：`--time 10`（10 分钟）
- **注意**：单个数字总是被解释为分钟，而不是秒

### 按格式的示例

```bash
# 2 小时
gbatch --time 2:00:00 long_simulation

# 30 分钟
gbatch --time 30 medium_task

# 5 分钟 30 秒
gbatch --time 5:30 quick_task

# 12 小时 45 分钟
gbatch --time 12:45:00 overnight_job
```

## 调度优势

### 时间限制改善调度优先级

当多个任务以相同优先级排队时，gflow 使用多因素调度算法：

1. **用户优先级**（主要因素）
2. **时间限制奖励**（次要因素）
3. **提交顺序**（平局破坏者）

**时间奖励如何工作**：

- **无限制任务**：获得最低调度奖励
- **有时间限制的任务**：获得更高的调度奖励
- **较短的任务**：在有时间限制的任务中获得更高的奖励

这意味着设置时间限制提供两个优势：
- ✅ **安全性**：防止失控任务无限期消耗资源
- ✅ **优先级**：与无限制任务竞争时任务运行更快

**示例调度顺序**：

```bash
# 假设所有任务的优先级=10 并按此顺序提交：

gbatch --priority 10 --time 10 quick.py        # 第 1 个运行（最短）
gbatch --priority 10 --time 1:00:00 medium.py  # 第 2 个运行（中等）
gbatch --priority 10 --time 8:00:00 long.py    # 第 3 个运行（长限制）
gbatch --priority 10 unlimited.py              # 第 4 个运行（无限制）
```

**关键见解**：即使是非常宽松的时间限制（例如 24 小时）也会给您的任务相对于相同优先级的无限制任务的优势。设置现实的时间限制是双赢！

### 调度优先级详情

时间奖励使用以下公式：
- 无时间限制：奖励 = 100
- 有时间限制：奖励 = 200-300（基于时长）
  - 非常短的任务（秒-分钟）：~300
  - 中等任务（小时）：~250
  - 长任务（≥24 小时）：~200

用户优先级乘以 1000，所以它总是主导调度决策。时间奖励仅在任务优先级相等时重要。

## 行为和执行

### 时间限制如何工作

1. **任务提交**：当您使用 `--time` 提交任务时，时间限制与任务元数据一起存储。

2. **执行开始**：当任务开始运行时，调度器记录开始时间。

3. **监控**：调度器每 5 秒检查一次所有运行中的任务是否超时。

4. **超时检测**：如果任务的运行时间超过其时间限制，调度器会：
   - 记录警告：`Job <id> has exceeded time limit, terminating...`
   - 向任务的 tmux 会话发送 `Ctrl-C`（优雅中断）
   - 将任务转换到 `Timeout` 状态
   - 记录完成时间

5. **终止后**：任务的输出保存在日志文件中，会话被清理。

### 优雅与强制终止

- **优雅终止**：gflow 首先发送 `Ctrl-C`（SIGINT），给任务机会：
  - 保存检查点
  - 关闭文件句柄
  - 清理临时文件
  - 记录最终状态

- **强制清理**：如果 tmux 会话不响应，当任务完全取消或守护进程停止时会被杀死。

### 准确性和时序

- **检查间隔**：5 秒（任务可能运行超过限制最多 5 秒）
- **容差**：任务在下一次检查检测到超时时立即终止
- **精度**：记录亚秒精度，但执行以 5 秒间隔进行

## 任务状态

### 超时状态（`TO`）

当任务超过其时间限制时，它转换到 `Timeout` 状态：

```bash
$ gqueue -j 42
JOBID    NAME             ST    TIME         TIMELIMIT
42       my-long-job      TO    00:10:05     00:10:00
```

关键特征：
- **状态代码**：`TO`（超时）
- **终端状态**：任务不会重新启动或继续
- **可区分**：不同于 `F`（失败）和 `CA`（已取消）
- **已记录**：超时事件记录在守护进程日志中

### 状态转换

```
Queued ──→ Running ──→ Finished
            │
            ├──→ Failed
            ├──→ Cancelled
            └──→ Timeout (new)
```

转换到 `Timeout` 的有效转换：
- ✅ `Running` → `Timeout`（超过时间限制）
- ❌ `Queued` → `Timeout`（不可能）
- ❌ `Finished` → `Timeout`（终端状态）

## 示例

### 示例 1：带时间限制的训练任务

```bash
# 提交 2 小时限制的训练任务
gbatch --time 2:00:00 \
       --gpus 1 \
       python train.py --epochs 100
```

**输出**：
```
Submitted batch job 42 (elegant-mountain-1234)
```

**检查状态**：
```bash
$ gqueue -j 42 -f JOBID,NAME,ST,TIME,TIMELIMIT
JOBID    NAME                   ST    TIME         TIMELIMIT
42       elegant-mountain-1234  R     00:15:23     02:00:00
```

### 示例 2：超时的任务

```bash
# 提交将超过其限制的任务
gbatch --time 0:10 \
       sleep 1000  # 将运行 1000 秒
```

**10 秒后**：
```bash
$ gqueue -j 43 -f JOBID,NAME,ST,TIME,TIMELIMIT
JOBID    NAME                ST    TIME         TIMELIMIT
43       quiet-river-5678    TO    00:00:13     00:00:10
```

**日志输出**（`~/.local/share/gflow/logs/43.log`）：
```
Line 1
Line 2
...
^C
```

### 示例 3：带时间限制的任务数组

```bash
# 提交任务数组，每个 30 分钟限制
gbatch --time 30 \
       --array 1-10 \
       python process.py --task \$GFLOW_ARRAY_TASK_ID
```

数组中的每个任务继承相同的 30 分钟时间限制。

### 示例 4：带时间限制的依赖链

```bash
# 任务 1：数据预处理（1 小时）
gbatch --time 1:00:00 \
       --name "preprocess" \
       python preprocess.py


# 任务 2：训练（4 小时），依赖于任务 1
gbatch --time 4:00:00 \
       --depends-on 1 \
       --name "training" \
       python train.py

# 任务 3：评估（30 分钟），依赖于任务 2
gbatch --time 30 \
       --depends-on 2 \
       --name "evaluation" \
       python evaluate.py \
```

### 示例 5：带时间限制的任务脚本

创建 `experiment.sh`：
```bash
#!/bin/bash
# GFLOW --time 3:00:00
# GFLOW --gpus 2
# GFLOW --priority 20

echo "Starting experiment at $(date)"
python run_experiment.py --config config.yaml
echo "Experiment finished at $(date)"
```

提交：
```bash
gbatch experiment.sh
```

从命令行覆盖时间限制：
```bash
# 这覆盖脚本的 3 小时限制
gbatch --time 1:00:00 experiment.sh
```

## 最佳实践

### 1. 设置现实的时间限制

- **估计运行时间**：在预期运行时间上增加 10-20% 的缓冲
- **考虑可变性**：考虑数据集大小、硬件性能
- **太短**：任务过早终止，浪费计算
- **太长**：无法帮助捕获失控任务

```bash
# 不好：太紧
gbatch --time 10 python train.py  # 训练需要 ~12 分钟

# 好：合理的缓冲
gbatch --time 15 python train.py  # 允许 25% 缓冲
```

### 2. 为所有生产任务使用时间限制

```bash
# 不好：无限制，可能永远运行
gbatch python train.py

# 好：始终指定限制
gbatch --time 4:00:00 python train.py
```

### 3. 实现检查点

时间限制与检查点配合效果最佳：

```python
# 您的训练脚本
import signal
import sys

def signal_handler(sig, frame):
    print('Received interrupt, saving checkpoint...')
    model.save_checkpoint('checkpoint.pth')
    sys.exit(0)

signal.signal(signal.SIGINT, signal_handler)

# 训练循环
for epoch in range(epochs):
    train_epoch()
    if epoch % 10 == 0:
        model.save_checkpoint(f'checkpoint_epoch_{epoch}.pth')
```

### 4. 监控时间使用

使用 `gqueue` 监控任务进度：

```bash
# 监控带时间限制的任务
watch -n 5 'gqueue -s Running -f JOBID,NAME,ST,TIME,TIMELIMIT'
```

### 5. 根据历史调整限制

运行任务后，分析其运行时间：

```bash
# 检查已完成任务的运行时间
gqueue -j 42 -f JOBID,TIME,TIMELIMIT

# 根据实际运行时间调整未来任务
```

### 6. 为不同阶段设置不同的限制

```bash
# 快速预处理
gbatch --time 10 --name "preprocess" python preprocess.py

# 长时间训练
gbatch --time 8:00:00 --depends-on <prep_id> --name "training" python train.py

# 快速评估
gbatch --time 5 --name "evaluation" --depends-on <train_id> python evaluate.py
```

## 显示时间限制

### 使用 `gqueue`

显示时间限制列：
```bash
# 在输出中包含 TIMELIMIT
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
```

**输出**：
```
JOBID    NAME                ST    TIME         TIMELIMIT
42       training-job        R     01:23:45     04:00:00
43       quick-task          CD    00:02:15     00:10:00
44       unlimited-job       R     00:45:12     UNLIMITED
```

### 时间显示格式

- **带天数**：`D-HH:MM:SS`（例如 `2-04:30:00` = 2 天、4.5 小时）
- **不带天数**：`HH:MM:SS`（例如 `04:30:00` = 4.5 小时）
- **无限制**：`UNLIMITED`（未设置时间限制）

### 按状态过滤

查看所有超时的任务：
```bash
gqueue -s Timeout -f JOBID,NAME,TIME,TIMELIMIT
```

## 常见问题

### Q：如果我不指定时间限制会发生什么？

**A**：任务运行时没有任何时间限制（`UNLIMITED`）。它将运行直到：
- 成功完成
- 由于错误而失败
- 您使用 `gcancel` 手动取消
- 系统崩溃或守护进程停止

### Q：我可以在提交后更改任务的时间限制吗？

**A**：目前不行。时间限制在提交时设置，无法为队列或运行中的任务修改。您需要：
1. 取消当前任务
2. 使用新的时间限制重新提交

### Q：最大时间限制是多少？

**A**：没有硬性最大值，但实际限制取决于：
- 系统稳定性（天/周）
- 资源可用性
- 您的具体用例

非常长限制的示例：
```bash
gbatch --time 168:00:00 week_long_simulation # 1 周
```

### Q：我的任务在超时时会保存其工作吗？

**A**：这取决于您的任务实现：
- **带 SIGINT 处理程序**：是的，如果您捕获 `Ctrl-C` 并保存状态
- **无处理程序**：可能不会，任务立即中断
- **最佳实践**：实现定期检查点

### Q：我如何查看任务为什么超时？

**A**：检查多个来源：

1. **任务状态**：
   ```bash
   gqueue -j <job_id>
   ```

2. **任务日志**：
   ```bash
   cat ~/.local/share/gflow/logs/<job_id>.log
   ```

3. **守护进程日志**（如果附加到 tmux）：
   ```bash
   tmux attach -t gflow_server
   ```

### Q：我可以为任务数组设置不同的时间限制吗？

**A**：目前，数组中的所有任务共享相同的时间限制。要有不同的限制：
- 单独提交任务，或
- 根据 `$GFLOW_ARRAY_TASK_ID` 在脚本中实现条件逻辑

### Q：超时（TO）和失败（F）之间的区别是什么？

| 方面 | 超时（TO） | 失败（F） |
|------|-----------|---------|
| **原因** | 超过时间限制 | 任务崩溃/错误 |
| **由谁发起** | 调度器 | 任务本身 |
| **退出代码** | SIGINT（130） | 可变（任务相关） |
| **计划** | 是的（达到限制） | 否（意外） |

### Q：时间限制包括队列时间吗？

**A**：不包括，仅计算运行时间。计时器在任务从 `Queued` 转换到 `Running` 状态时开始。

```
Queued（不计算）→ Running（计时器开始）→ Timeout/Finished
```

### Q：超时执行的准确性如何？

**A**：在 5 秒内。调度器每 5 秒检查一次，所以：
- **限制**：10:00
- **实际终止**：10:00 到 10:05 之间

对于大多数用例，这种准确性是足够的。

### Q：如果守护进程在任务运行时重新启动会发生什么？

**A**：时间限制被保留：
1. 任务状态（包括开始时间）保存到磁盘
2. 守护进程重新启动时，它重新加载任务状态
3. 时间限制检查自动恢复
4. 在停机期间超过限制的任务将在下一次检查时被捕获

### Q：我可以看到运行中任务的剩余时间吗？

**A**：不直接，但您可以计算：

```bash
# 显示 TIME 和 TIMELIMIT 列
gqueue -j <job_id> -f JOBID,NAME,TIME,TIMELIMIT

# 计算：TIMELIMIT - TIME = 剩余时间
```

未来的增强可能会添加 `REMAINING` 列。

## 故障排除

### 任务比预期更早终止

**可能原因**：
1. **错误的时间格式**：
   - ❌ `--time 30` 认为是秒（实际上是 30 分钟）
   - ✅ `--time 0:30` 表示 30 秒

2. **时间限制太严格**：检查前一个任务的实际运行时间

3. **任务因其他原因失败**：检查日志和任务状态

### 任务在时间限制时不终止

**可能原因**：
1. **未设置时间限制**：使用 `gqueue -f TIMELIMIT` 验证
2. **守护进程未运行**：检查 `ginfo`
3. **任务不在运行状态**：时间限制仅适用于运行中的任务

### 时间限制未在 `gqueue` 中显示

```bash
# 确保在格式中包含 TIMELIMIT
gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT

# 或检查您的 gflow 版本的默认列
```

## 实现详情

对于开发者和高级用户：

### 架构

- **存储**：`Job` 结构中的 `time_limit` 字段为 `Option<Duration>`
- **检查**：调度器循环每 5 秒
- **方法**：`Job::has_exceeded_time_limit()` 比较运行时间与限制
- **终止**：`send_ctrl_c()` → 转换到 `Timeout` 状态

### 状态持久化

时间限制持久化在 `~/.local/share/gflow/state.json` 中：

```json
{
  "id": 42,
  "time_limit": {
    "secs": 3600,
    "nanos": 0
  },
  "started_at": {
    "secs_since_epoch": 1234567890,
    "nanos_since_epoch": 0
  }
}
```

### 调度器代码

位于 `src/bin/gflowd/scheduler.rs:242-267`，超时检查逻辑在每个调度器周期运行。

## 相关功能

- **任务依赖**：与 `--depends-on` 结合用于复杂工作流
- **任务优先级**：与 `--priority` 一起用于重要的时间敏感任务
- **任务数组**：将时间限制应用于并行任务批
- **输出日志**：所有输出通过 `pipe-pane` 捕获到日志文件

## 另见

- [介绍](/) - 主文档
- [任务依赖](./job-dependencies) - 管理任务依赖
- [GPU 管理](./gpu-management) - GPU 资源管理
- GitHub Issues - 报告问题或请求功能
