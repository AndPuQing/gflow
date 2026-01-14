# gcancel 命令参考

完整的 `gcancel` 命令参考 - gflow 的任务取消工具。

## 概要

```bash
gcancel [OPTIONS] [IDS...]
gcancel --dry-run [IDS...]
gcancel --finish <ID>
gcancel --fail <ID>
```

## 描述

`gcancel` 取消 gflow 队列中的一个或多个任务。它支持单个任务 ID、范围和列表。该命令还提供了一个试运行模式来预览取消的影响。

## 选项

### 任务选择

#### `[IDS...]`

要取消的任务 ID。支持多种格式：

**格式**：
- 单个 ID：`42`
- 多个 ID：`1 2 3` 或 `1,2,3`
- 范围：`1-5`（ID 1, 2, 3, 4, 5）
- 混合：`1,3,5-7,10`

**示例**：
```bash
# 取消单个任务
gcancel 42

# 取消多个任务
gcancel 1 2 3
gcancel 1,2,3

# 取消范围
gcancel 1-5

# 取消混合
gcancel 1,3,5-7,10
```

### 预览模式

#### `--dry-run`

预览取消而不执行。

**特性**：
- 显示将取消哪些任务
- 识别将受影响的依赖任务
- 在取消前验证任务 ID
- 安全运行 - 不做任何更改

**示例**：
```bash
$ gcancel --dry-run 1
Would cancel job 1 (data-prep)

⚠️  Warning: The following jobs depend on job 1:
  - Job 2 (train-model)
  - Job 3 (evaluate)

These jobs will never start if job 1 is cancelled.

To proceed with cancellation, run:
  gcancel 1
```

**用例**：
- 取消前检查影响
- 验证任务 ID 正确
- 理解依赖链
- 规划清理操作

### 内部状态管理（隐藏）

这些选项由 gflow 内部使用，不适合直接用户交互。

#### `--finish <ID>`

标记任务为已完成（仅内部使用）。

**示例**：
```bash
gcancel --finish 42
```

**注意**：由系统用于转换任务状态。不建议手动使用。

#### `--fail <ID>`

标记任务为失败（仅内部使用）。

**示例**：
```bash
gcancel --fail 42
```

**注意**：由系统用于转换任务状态。不建议手动使用。

### 全局选项

#### `--config <PATH>`

使用自定义配置文件（隐藏选项）。

**示例**：
```bash
gcancel --config /path/to/custom.toml 42
```

#### `--help`, `-h`

显示帮助消息。

```bash
$ gcancel --help
<!-- cmdrun gcancel --help -->
```

#### `--version`, `-V`

显示版本信息。

```bash
$ gcancel --version
<!-- cmdrun gcancel --version -->
```

## 行为

### 成功取消

当任务成功取消时：

1. 任务状态变为 `Cancelled`（CA）
2. 如果任务正在运行：
   - tmux 会话接收 `Ctrl-C`（SIGINT）
   - 任务进程被优雅中断
   - 会话被清理
3. 如果任务在队列中：
   - 任务从运行队列中移除
   - 状态立即改变
4. 输出被捕获到日志文件
5. 完成时间被记录

**示例**：
```bash
$ gcancel 42
Job 42 cancelled successfully

$ gqueue -j 42
JOBID    NAME      ST    TIME
42       my-job    CA    00:05:23
```

### 依赖任务

取消任务会影响依赖任务：

- 依赖任务保持在 `Queued` 状态
- 它们**永远不会**自动启动
- 你必须手动取消它们

**示例**：
```bash
# 任务 2 依赖任务 1
$ gqueue -t
JOBID    NAME      ST
1        prep      R
└─ 2     train     PD

# 取消任务 1
$ gcancel 1
Job 1 cancelled

# 任务 2 现在是孤立的
$ gqueue -t
JOBID    NAME      ST
1        prep      CA
└─ 2     train     PD    # 永远不会启动

# 必须手动取消任务 2
$ gcancel 2
```

### 已完成的任务

无法取消已完成的任务：

```bash
$ gcancel 42
Error: Job 42 is already in terminal state (Finished)
```

**终止状态**（无法取消）：
- `Finished`（CD）
- `Failed`（F）
- `Cancelled`（CA）
- `Timeout`（TO）

### 不存在的任务

```bash
$ gcancel 999
Error: Job 999 not found
```

## 示例

### 基本取消

```bash
# 取消单个任务
gcancel 42

# 取消多个任务
gcancel 1 2 3
gcancel 1,2,3

# 取消范围
gcancel 10-20

# 取消混合
gcancel 1,5,10-15,20
```

### 取消前预览

```bash
# 检查会发生什么
gcancel --dry-run 5

# 仔细阅读输出
# 如果可以接受，继续
gcancel 5
```

### 取消管道

```bash
# 查看管道
$ gqueue -t
JOBID    NAME      ST
1        prep      R
├─ 2     train-a   PD
└─ 3     train-b   PD

# 取消整个管道
gcancel 1,2,3

# 或分别取消父任务和子任务
gcancel 1
gcancel 2 3
```

### 取消所有运行中的任务

```bash
# 获取运行中的任务 ID
RUNNING=$(gqueue -s Running -f JOBID | tail -n +2 | tr '\n' ',' | sed 's/,$//')

# 取消它们
gcancel $RUNNING
```

### 取消队列中的任务

```bash
# 获取队列中的任务 ID
QUEUED=$(gqueue -s Queued -f JOBID | tail -n +2)

# 逐个取消
for job in $QUEUED; do
    gcancel $job
done
```

### 条件取消

```bash
# 如果任务耗时过长则取消
JOB_ID=42
ELAPSED=$(gqueue -j $JOB_ID -f TIME | tail -n 1 | cut -d: -f1)

if [ "$ELAPSED" -gt 2 ]; then
    echo "Job taking too long, cancelling..."
    gcancel $JOB_ID
fi
```

## 常见模式

### 取消并重新提交

```bash
# 取消旧任务
gcancel 42

# 使用修正重新提交
gbatch --gpus 1 --time 2:00:00 python train.py --fixed
```

### 取消失败的依赖

```bash
# 查找失败的任务
$ gqueue -s Failed
JOBID    NAME      ST
5        prep      F

# 取消依赖任务（它们无论如何都不会启动）
$ gqueue -t | grep -A10 "^5"
5        prep      F
└─ 6     train     PD

$ gcancel 6
```

### 紧急停止

```bash
# 停止所有运行中和队列中的任务
gcancel $(gqueue -s Running,Queued -f JOBID | tail -n +2)
```

### 选择性取消

```bash
# 取消低优先级的队列中任务
gqueue -s Queued -r priority -f JOBID,PRIORITY | awk '$2 < 10 {print $1}' | xargs gcancel
```

## 集成示例

### 脚本：安全取消

```bash
#!/bin/bash
# safe_cancel.sh - 带依赖检查的取消

JOB_ID=$1

# 检查依赖
gcancel --dry-run $JOB_ID

read -p "Proceed with cancellation? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    gcancel $JOB_ID
    echo "Job $JOB_ID cancelled"
else
    echo "Cancellation aborted"
fi
```

### 脚本：取消管道

```bash
#!/bin/bash
# cancel_pipeline.sh - 取消管道中的所有任务

ROOT_JOB=$1

# 获取所有依赖任务
JOBS=$(gqueue -t | awk -v root=$ROOT_JOB '
    $1 == root || seen {
        seen = 1
        print $1
    }
')

echo "Jobs to cancel: $JOBS"
gcancel $JOBS
```

### 脚本：超时监视器

```bash
#!/bin/bash
# timeout_watcher.sh - 取消超过预期时间的任务

MAX_TIME=120  # 2 小时（分钟）

gqueue -s Running -f JOBID,TIME | tail -n +2 | while read -r jobid time; do
    # 将时间转换为分钟
    IFS=: read -r h m s <<< "$time"
    minutes=$((10#$h * 60 + 10#$m))

    if [ $minutes -gt $MAX_TIME ]; then
        echo "Job $jobid exceeded $MAX_TIME minutes, cancelling..."
        gcancel $jobid
    fi
done
```

## 故障排除

### 问题：无法取消任务

**可能原因**：
1. 任务已处于终止状态
2. 任务 ID 不存在
3. 权限问题

**解决方案**：
```bash
# 检查任务状态
gqueue -j <job_id> -f JOBID,ST

# 验证任务存在
gqueue -a | grep <job_id>

# 检查守护进程状态
ginfo
```

### 问题：依赖任务未被取消

**预期行为**：gcancel 仅取消指定的任务，不取消依赖任务。

**解决方案**：手动取消依赖任务：
```bash
# 使用试运行查看依赖任务
gcancel --dry-run 1

# 取消父任务和子任务
gcancel 1,2,3
```

### 问题：取消后任务仍在运行

**可能原因**：
1. 任务优雅处理 SIGINT（保存状态）
2. tmux 会话清理延迟
3. 任务进程无响应

**解决方案**：
```bash
# 等待几秒
sleep 5
gqueue -j <job_id>

# 检查 tmux 会话
tmux ls

# 如果需要，强制关闭 tmux 会话
tmux kill-session -t <session_name>
```

### 问题：范围解析错误

**示例**：
```bash
gcancel 1-5,10
```

**解决方案**：检查范围语法：
- 范围：`1-5`（有效）
- 列表：`1,2,3`（有效）
- 混合：`1-5,10`（取决于实现）

## 最佳实践

1. **对重要取消使用试运行**
   ```bash
   gcancel --dry-run 42
   gcancel 42
   ```

2. **取消父任务前检查依赖**
   ```bash
   gqueue -t
   ```

3. **在开发/测试期间优雅取消**
   - 任务可以保存检查点
   - 日志被保留

4. **取消父任务时清理依赖任务**
   ```bash
   gcancel 1,2,3  # 父任务和子任务
   ```

5. **监控取消以确保完成**
   ```bash
   gcancel 42
   watch -n 1 'gqueue -j 42'
   ```

6. **记录取消以进行审计**
   ```bash
   echo "$(date): Cancelled job 42" >> ~/gflow-cancellations.log
   gcancel 42
   ```

7. **使用任务名称识别要取消的任务**
   ```bash
   gqueue -N "old-experiment*"
   gcancel <identified_ids>
   ```

8. **避免取消系统任务**（如果有）
   - 对自动取消脚本要小心

## 错误消息

### 常见错误

```bash
# 任务未找到
Error: Job 999 not found
```

```bash
# 任务已处于终止状态
Error: Job 42 is already in terminal state (Finished)
```

```bash
# 无效的任务 ID
Error: Invalid job ID: abc
```

```bash
# 未提供任务 ID
Error: No job IDs specified
Usage: gcancel [OPTIONS] [IDS...]
```

```bash
# 守护进程未运行
Error: Could not connect to gflowd (connection refused)
```

## 退出代码

| 代码 | 含义 |
|------|------|
| 0 | 成功 |
| 1 | 错误（任务未找到、已处于终止状态等） |
| 2 | 无效参数 |

## 参见

- [gbatch](./gbatch-reference) - 任务提交参考
- [gqueue](./gqueue-reference) - 任务队列参考
- [ginfo](./ginfo-reference) - 调度器检查参考
- [快速参考](./quick-reference) - 命令速查表
- [任务提交](../user-guide/job-submission) - 任务提交指南
- [任务依赖](../user-guide/job-dependencies) - 依赖管理
