# 任务提交

本指南涵盖了使用 `gbatch` 提交任务的所有方面，从基本用法到高级功能。

## 概述

`gbatch` 是 gflow 的任务提交工具，类似于 Slurm 的 `sbatch`。它支持直接命令执行和基于脚本的任务提交。

## 基本用法

### 提交命令

提交任务的最简单方法是直接提供命令：

```bash
gbatch python script.py
```

输出：
```
Submitted batch job 1 (silent-pump-6338)
```

**无需引号** 用于简单命令！参数会自动连接：

```bash
gbatch python train.py --epochs 100 --lr 0.01
```

### 命令参数安全性

gflow 使用 shell 转义自动处理命令参数中的特殊字符：

```bash
# 包含空格的参数
gbatch python script.py --message "Hello World"

# 包含特殊字符的参数
gbatch python script.py --pattern 'test_*.py'

# 复杂参数
gbatch bash -c 'echo $USER && python script.py'
```

**工作原理**：
- 命令参数在执行前会被正确转义
- 防止 shell 注入和意外的命令解释
- 安全处理空格、引号和通配符等特殊字符
- 使用 `shell-escape` 库确保安全性

**最佳实践**：虽然 gflow 会自动处理转义，但仍建议：
- 在本地先测试复杂命令
- 使用显式引号以提高清晰度
- 避免过于复杂的内联命令（改用脚本文件）

### 提交脚本

创建脚本文件并提交：

```bash
# 创建脚本
cat > my_job.sh << 'EOF'
#!/bin/bash
echo "Hello from gflow!"
python train.py
EOF

# 设置可执行权限
chmod +x my_job.sh

# 提交
gbatch my_job.sh
```

## 资源分配

### GPU 请求

为任务请求 GPU：

```bash
# 请求 1 个 GPU
gbatch --gpus 1 python train.py

# 请求 2 个 GPU
gbatch --gpus 2 python multi_gpu_train.py
```

调度器会自动将 `CUDA_VISIBLE_DEVICES` 设置为分配的 GPU。

**检查 GPU 分配**：
```bash
$ gqueue -f JOBID,NAME,NODES,NODELIST
JOBID    NAME                NODES    NODELIST(REASON)
42       silent-pump-6338    1        0
43       brave-river-1234    2        1,2
```

### Conda 环境

在运行任务前激活 conda 环境：

```bash
gbatch --conda-env myenv python script.py
```

这等同于运行：
```bash
conda activate myenv
python script.py
```

## 任务调度选项

### 优先级

控制任务相对于其他任务的运行时间：

```bash
# 高优先级（首先运行）
gbatch --priority 100 python urgent.py

# 默认优先级
gbatch python normal.py  # priority = 10

# 低优先级（最后运行）
gbatch --priority 1 python background.py
```

**优先级详情**：
- 范围：0-255
- 默认值：10
- 值越高 = 优先级越高
- 任务根据多因素优先级系统进行调度（见下文）

**调度优先级层次**：

当资源可用时，gflow 使用三级优先级系统调度任务：

1. **用户优先级**（主要）：具有更高 `--priority` 值的任务首先运行
2. **时间限制奖励**（次要）：在优先级相同的任务中：
   - 有时间限制的任务优于无限制任务
   - 较短的任务优于较长的任务
3. **提交顺序**（第三级）：较早提交的任务首先运行（FIFO）

**示例**：

```bash
# 当 GPU 可用时，这些任务将按以下顺序运行：

# 第 1 个：高优先级，即使无限制
gbatch --priority 20 python urgent.py

# 第 2 个：相同优先级，但 10 分钟限制优于无限制
gbatch --priority 10 --time 10 python quick.py

# 第 3 个：相同优先级，但 1 小时限制（最先提交）
gbatch --priority 10 --time 1:00:00 python train1.py  # Job ID 100

# 第 4 个：相同优先级和限制，但提交较晚
gbatch --priority 10 --time 1:00:00 python train2.py  # Job ID 101

# 第 5 个：相同优先级，无限制（最先提交）
gbatch --priority 10 python long1.py  # Job ID 102

# 第 6 个：相同优先级，无限制（提交较晚）
gbatch --priority 10 python long2.py  # Job ID 103
```

**关键见解**：
- 设置 `--time` 不仅可以防止失控任务，还能改善调度优先级
- 较短的时间限制获得轻微偏好，鼓励准确的估计
- 提交顺序在其他条件相同时充当公平的平局破坏者

### 时间限制

设置任务的最大运行时间：

```bash
# 30 分钟
gbatch --time 30 python quick.py

# 2 小时
gbatch --time 2:00:00 python train.py

# 5 分钟 30 秒
gbatch --time 5:30 python test.py
```

详见 [时间限制](./time-limits)。

### 任务名称

默认情况下，任务获得自动生成的名称（例如"silent-pump-6338"）。您可以指定自定义名称：

```bash
gbatch --name "my-training-run" python train.py
```

**注意**：`--name` 选项用于自定义命名。如果未指定，将生成随机名称。

## 任务依赖

使任务等待其他任务完成：

```bash
# 任务 1：预处理
gbatch --name "prep" python preprocess.py
# 返回：Submitted batch job 1

# 任务 2：训练（等待任务 1）
gbatch --depends-on 1 --name "train" python train.py

# 任务 3：评估（等待任务 2）
gbatch --depends-on 2 --name "eval" python evaluate.py
```

详见 [任务依赖](./job-dependencies)。

## 更新排队任务

您可以使用 `gjob update` 命令更新仍在排队或暂停状态的任务参数。这对于修正错误、调整资源需求或在任务开始运行前更改优先级非常有用。

### 何时允许更新

- **排队任务**：可以更新所有参数
- **暂停任务**：可以更新所有参数
- **运行中任务**：无法更新（任务已在执行）
- **已完成任务**：无法更新（任务已结束）

### 可更新的内容

您可以更新以下任务参数：

- 命令或脚本
- GPU 需求
- Conda 环境
- 优先级
- 时间和内存限制
- 依赖关系
- 模板参数
- 组内最大并发任务数

### 基本用法

```bash
# 更新任务 123 的 GPU 数量
gjob update 123 --gpus 2

# 更新优先级
gjob update 123 --priority 15

# 同时更新多个参数
gjob update 123 --gpus 4 --priority 20 --time-limit 02:00:00

# 更新多个任务
gjob update 123,124,125 --priority 10
```

### 更新资源

```bash
# 更改 GPU 分配
gjob update 123 --gpus 4

# 更新时间限制
gjob update 123 --time-limit 04:30:00

# 更新内存限制
gjob update 123 --memory-limit 32G

# 清除时间限制（无限制）
gjob update 123 --clear-time-limit

# 清除内存限制
gjob update 123 --clear-memory-limit
```

### 更新依赖关系

```bash
# 更新依赖关系（AND 逻辑 - 所有任务必须完成）
gjob update 123 --depends-on 100,101,102

# 使用显式 AND 逻辑更新
gjob update 123 --depends-on-all 100,101,102

# 使用 OR 逻辑更新（任意一个完成即可）
gjob update 123 --depends-on-any 100,101

# 启用依赖失败时自动取消
gjob update 123 --auto-cancel-on-dep-failure

# 禁用自动取消
gjob update 123 --no-auto-cancel-on-dep-failure
```

### 更新命令或脚本

```bash
# 更新命令
gjob update 123 --command "python train.py --epochs 100"

# 更新脚本路径
gjob update 123 --script /path/to/new_script.sh

# 更新 conda 环境
gjob update 123 --conda-env myenv

# 清除 conda 环境
gjob update 123 --clear-conda-env
```

### 更新模板参数

```bash
# 更新单个参数
gjob update 123 --param learning_rate=0.001

# 更新多个参数
gjob update 123 --param batch_size=64 --param epochs=100
```

### 常见用例

**修正已提交任务中的错误**：
```bash
# 糟糕，提交时 GPU 数量错了
gbatch --gpus 1 python train.py
# 返回：Submitted batch job 123

# 在运行前修正
gjob update 123 --gpus 4
```

**适应变化的条件**：
```bash
# 有更多 GPU 可用了，增加分配
gjob update 123 --gpus 8

# 系统繁忙，降低优先级让紧急任务先运行
gjob update 123 --priority 5
```

**迭代优化**：
```bash
# 从保守估计开始
gbatch --time-limit 01:00:00 python experiment.py
# 返回：Submitted batch job 123

# 意识到需要更多时间
gjob update 123 --time-limit 04:00:00
```

### 限制和约束

- 无法更新任务 ID、提交者或组 ID（不可变）
- 无法更新运行中或已完成的任务
- 依赖关系更新不能创建循环依赖
- 优先级必须在 0-255 之间
- 所有依赖任务 ID 必须存在

### 错误处理

如果更新失败，您会看到清晰的错误消息：

```bash
$ gjob update 123 --gpus 4
Error updating job 123: Job 123 is in state 'Running' and cannot be updated.
Only queued or held jobs can be updated.

$ gjob update 123 --depends-on 123
Error updating job 123: Circular dependency detected: Job 123 depends on Job 123,
which has a path back to Job 123
```

## 任务数组

并行运行多个类似的任务：

```bash
# 创建 10 个任务，任务 ID 为 1-10
gbatch --array 1-10 python process.py --task '$GFLOW_ARRAY_TASK_ID'
```

**工作原理**：
- 创建 10 个独立任务
- 每个任务的 `$GFLOW_ARRAY_TASK_ID` 设置为其任务编号
- 所有任务共享相同的资源需求
- 适用于参数扫描、数据处理等

**不同参数的示例**：
```bash
gbatch --array 1-5 --gpus 1 --time 2:00:00 \
       python train.py --lr '$(echo "0.001 0.01 0.1 0.5 1.0" | cut -d" " -f$GFLOW_ARRAY_TASK_ID)'
```

**环境变量**：
- `GFLOW_ARRAY_TASK_ID`：数组任务的任务 ID（1、2、3、...）
- 对于非数组任务设置为 0

## 脚本指令

您可以使用 `# GFLOW` 指令在脚本中嵌入任务需求，而不是使用命令行选项：

```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 2:00:00
# GFLOW --priority 20
# GFLOW --conda-env myenv

echo "Starting training..."
python train.py --epochs 100
echo "Training complete!"
```

提交脚本：
```bash
gbatch my_script.sh
```

**指令优先级**：
- 命令行参数覆盖脚本指令
- 示例：`gbatch --time 1:00:00 my_script.sh` 覆盖脚本中的 `--time` 指令

**支持的指令**：
- `# GFLOW --gpus <N>`
- `# GFLOW --time <TIME>`
- `# GFLOW --priority <N>`
- `# GFLOW --conda-env <ENV>`
- `# GFLOW --depends-on <ID>`

## 创建脚本模板

使用 `gbatch new` 创建任务脚本模板：

```bash
$ gbatch new my_job
```

这会创建 `my_job.sh`，包含模板：
```bash
#!/bin/bash
# GFLOW --gpus 0
# GFLOW --time 1:00:00
# GFLOW --priority 10

# Your commands here
echo "Job started at $(date)"

# Add your actual commands
# python script.py

echo "Job finished at $(date)"
```

编辑模板并提交：
```bash
# 编辑脚本
vim my_job.sh

# 设置可执行权限
chmod +x my_job.sh

# 提交
gbatch my_job.sh
```

### 自动模板生成

任务脚本模板是**自动生成的**，来自 `gbatch` CLI 定义，以确保它始终反映可用选项：

- **模板源**：模板从 `src/bin/gbatch/cli.rs` 生成
- **自动同步**：当命令行选项更改时，预提交钩子会自动重新生成模板
- **始终最新**：您总是在模板中获得最新的可用选项

**对于开发者**：详见 `scripts/README.md` 了解模板生成的工作原理。

## 环境变量

gflow 在您的任务中自动设置这些环境变量：

| 变量 | 描述 | 示例 |
|------|------|------|
| `CUDA_VISIBLE_DEVICES` | 分配给任务的 GPU ID | `0,1` |
| `GFLOW_ARRAY_TASK_ID` | 数组任务的任务 ID（非数组为 0） | `5` |

**使用示例**：
```bash
#!/bin/bash
echo "Using GPUs: $CUDA_VISIBLE_DEVICES"
echo "Array task ID: $GFLOW_ARRAY_TASK_ID"
python train.py
```

## 输出和日志

任务输出会自动捕获到日志文件：

**日志位置**：`~/.local/share/gflow/logs/<job_id>.log`

**查看日志**：
```bash
# 查看已完成任务的日志
cat ~/.local/share/gflow/logs/42.log

# 跟踪运行中的任务日志
tail -f ~/.local/share/gflow/logs/42.log
```

**附加到运行中的任务**（通过 tmux）：
```bash
# 获取任务会话名称
gqueue -f JOBID,NAME

# 附加到会话
tmux attach -t <session_name>

# 分离而不停止（Ctrl-B，然后 D）
```

## 高级示例

### 参数扫描

测试多个超参数：

```bash
# 提交多个训练运行
for lr in 0.001 0.01 0.1; do
    gbatch --gpus 1 --time 4:00:00 \
           --name "train-lr-$lr" \
           python train.py --lr $lr
done
```

### 带依赖的管道

```bash
# 步骤 1：数据预处理
gbatch --time 30 python preprocess.py

# 步骤 2：训练
gbatch --time 4:00:00 --gpus 1 --depends-on @ python train.py

# 步骤 3：评估
gbatch --time 10 --depends-on @ python evaluate.py
```

`@` 符号引用最近提交的任务，使管道简洁清晰。

### 多阶段任务脚本

```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 8:00:00

set -e  # 出错时退出

echo "Stage 1: Data preparation"
python prepare_data.py

echo "Stage 2: Model training"
python train.py --checkpoint model.pth

echo "Stage 3: Evaluation"
python evaluate.py --model model.pth

echo "All stages complete!"
```

### 条件任务提交

```bash
#!/bin/bash
# 仅在前一个任务成功时提交任务

PREV_JOB=42
STATUS=$(gqueue -j $PREV_JOB -f ST | tail -n 1)

if [ "$STATUS" = "CD" ]; then
    gbatch python next_step.py
else
    echo "Previous job not completed successfully"
fi
```

## 常见模式

### 长时间运行带检查点

```python
# train.py 带检查点支持
import signal
import sys

def save_checkpoint():
    print("Saving checkpoint...")
    # 保存模型状态
    torch.save(model.state_dict(), 'checkpoint.pth')

def signal_handler(sig, frame):
    save_checkpoint()
    sys.exit(0)

signal.signal(signal.SIGINT, signal_handler)

# 训练循环
for epoch in range(epochs):
    train_epoch()
    if epoch % 10 == 0:
        save_checkpoint()
```

使用时间限制提交：
```bash
gbatch --time 8:00:00 --gpus 1 python train.py
```

### GPU 利用率检查

```bash
#!/bin/bash
# GFLOW --gpus 1

echo "Allocated GPUs: $CUDA_VISIBLE_DEVICES"
nvidia-smi --query-gpu=index,name,memory.total --format=csv
python train.py
```

## 验证和错误处理

`gbatch` 在接受提交前验证您的提交：

**常见验证错误**：
- **无效依赖**：任务 ID 不存在
  ```
  Error: Dependency job 999 not found
  ```

- **循环依赖**：任务依赖于自身或创建循环
  ```
  Error: Circular dependency detected
  ```

- **无效时间格式**：格式错误的时间规范
  ```
  Error: Invalid time format. Use HH:MM:SS, MM:SS, or MM
  ```

- **脚本未找到**：文件不存在
  ```
  Error: Script file not found: missing.sh
  ```

## 提示和最佳实践

1. **始终设置时间限制** 用于生产任务以防止失控进程
2. **使用有意义的名称** 便于任务跟踪
3. **在本地测试脚本** 提交前
4. **添加错误处理** （`set -e`）在 bash 脚本中
5. **实现检查点** 用于长时间运行的任务
6. **使用任务数组** 用于并行独立任务
7. **检查依赖** 提交依赖任务前
8. **监控 GPU 使用** 请求多个 GPU 时
9. **使用 conda 环境** 用于可重现性
10. **添加日志** 到您的脚本以便于调试

## 故障排除

### 问题：任务提交失败，显示"dependency not found"

**解决方案**：验证依赖任务存在：
```bash
gqueue -j <dependency_id>
```

### 问题：任务没有获得 GPU

**检查**：
1. 您请求了 GPU 吗？`--gpus 1`
2. GPU 可用吗？`ginfo`
3. 其他任务在使用所有 GPU 吗？`gqueue -s Running -f NODES,NODELIST`

### 问题：Conda 环境未激活

**检查**：
1. 环境名称正确：`conda env list`
2. Conda 在您的 shell 中初始化
3. 检查任务日志中的激活错误

### 问题：脚本不可执行

**解决方案**：
```bash
chmod +x my_script.sh
gbatch my_script.sh
```

## 参考

**完整命令语法**：
```bash
gbatch [OPTIONS] <SCRIPT>
gbatch [OPTIONS] <COMMAND> [ARGS...]
```

**所有选项**：
- `--gpus <N>` 或 `-g <N>`：GPU 数量
- `--time <TIME>` 或 `-t <TIME>`：时间限制
- `--priority <N>`：任务优先级（0-255，默认：10）
- `--depends-on <ID>`：任务依赖
- `--conda-env <ENV>` 或 `-c <ENV>`：Conda 环境
- `--array <SPEC>`：任务数组（例如"1-10"）
- `--name <NAME>`：自定义任务名称
- `--config <PATH>`：自定义配置文件（隐藏）

**获取帮助**：
```bash
$ gbatch --help
<!-- cmdrun gbatch --help -->
```

## 另见

- [时间限制](./time-limits) - 详细的时间限制文档
- [任务依赖](./job-dependencies) - 高级依赖工作流
- [GPU 管理](./gpu-management) - GPU 分配和监控
- [快速参考](../reference/quick-reference) - 命令速查表
