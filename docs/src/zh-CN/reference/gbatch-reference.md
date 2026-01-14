# gbatch 命令参考

完整的 `gbatch` 命令参考 - gflow 的任务提交工具。

## 概要

```bash
gbatch [OPTIONS] [COMMAND...]
gbatch [OPTIONS] [SCRIPT]
gbatch new <NAME>
```

## 描述

`gbatch` 向 gflow 调度器提交任务。它支持基于脚本和直接命令执行，使用位置参数，并提供广泛的资源分配、调度和任务依赖选项。

## 模式

### 脚本提交

提交脚本文件：

```bash
gbatch [OPTIONS] <SCRIPT>
```

脚本可以包含 `# GFLOW` 指令来指定任务参数。

### 直接命令

使用位置参数直接执行命令：

```bash
gbatch [OPTIONS] <COMMAND> [ARGS...]
```

命令及其参数在所有选项之后。命令行选项优先于脚本指令。

### 模板创建

创建新的任务脚本模板：

```bash
gbatch new <NAME>
```

创建 `<NAME>.sh` 文件，包含模板结构。

## 选项

### 任务提交

#### `<SCRIPT>`

要执行的脚本文件路径。

**示例**：
```bash
gbatch my_job.sh
gbatch ./scripts/train.sh
gbatch /absolute/path/to/job.sh
```

**要求**：
- 文件必须存在
- 应该是可执行的（`chmod +x`）
- 可以包含 `# GFLOW` 指令

**互斥**于直接命令执行。

#### 直接命令执行

当未提供脚本时，选项之后的所有参数都被视为要执行的命令。

**示例**：
```bash
gbatch python train.py
gbatch echo 'Hello'; sleep 10
gbatch nvidia-smi
```

**注意**：
- 在 bash shell 中执行
- 可以包含多个命令（使用 `;` 或 `&&`）
- 互斥于 `<SCRIPT>`
- 复杂参数的命令应该适当引用

### 资源分配

#### `--gpus <N>`, `-g <N>`

请求的 GPU 数量。

**示例**：
```bash
# 请求 1 个 GPU
gbatch --gpus 1 python train.py

# 请求 2 个 GPU
gbatch -g 2 python multi_gpu_train.py

# 不使用 GPU（默认）
gbatch python cpu_task.py
```

**行为**：
- 默认：0（无 GPU）
- 自动设置 `CUDA_VISIBLE_DEVICES`
- 如果 GPU 不足，任务等待
- 参见 [GPU 管理](../user-guide/gpu-management)

#### `--conda-env <ENV>`

要激活的 Conda 环境。

**示例**：
```bash
gbatch --conda-env myenv python script.py
gbatch --conda-env pytorch_env my_training.sh
```

**行为**：
- 在任务前运行 `conda activate <ENV>`
- 需要在 shell 中初始化 conda
- 如果环境不存在，任务失败

**自动检测**（仅限直接命令模式）：
- 如果未指定 `--conda-env`，gflow 会自动检测当前活跃的 conda 环境
- 使用 `CONDA_DEFAULT_ENV` 环境变量
- 仅在使用直接命令执行时适用，不适用于提交脚本
- 显式 `--conda-env` 始终优先于自动检测

```bash
# 使用自动检测（使用当前活跃的 conda 环境）
conda activate myenv
gbatch python script.py  # 将使用 'myenv'

# 显式覆盖（忽略活跃环境）
conda activate myenv
gbatch --conda-env otherenv python script.py  # 使用 'otherenv'

# 无 conda 环境
conda deactivate
gbatch python script.py  # 无 conda 激活
```

### 调度选项

#### `--priority <N>`

任务优先级（0-255，默认：10）。

**示例**：
```bash
# 高优先级（首先运行）
gbatch --priority 100 python urgent.py

# 默认优先级
gbatch python normal.py  # priority = 10

# 低优先级（最后运行）
gbatch --priority 1 python background.py
```

**行为**：
- 值越高 = 优先级越高
- 资源释放时，高优先级任务首先启动
- 不抢占运行中的任务

#### `--depends-on <ID>`

单个任务依赖。任务等待指定任务成功完成。

**示例**：
```bash
# 任务 1
gbatch python preprocess.py
# 返回任务 ID 1

# 任务 2（依赖 1）
gbatch --depends-on 1 python train.py

# 等效的简写，使用最后一次提交
gbatch --depends-on @ python train.py

# 两次提交之前
gbatch --depends-on @~2 python evaluate.py
```

**行为**：
- 任务等待依赖达到 `Finished` 状态
- 如果依赖失败，此任务默认自动取消（参见 `--no-auto-cancel`）
- 参见 [任务依赖](../user-guide/job-dependencies)
- 简写值解析为 `gbatch` 记录的最近任务 ID

**验证**：
- 依赖任务必须存在
- 不允许循环依赖
- 不能依赖自己
- `@~N` 需要至少 `N` 次之前的提交

**冲突选项**：`--depends-on-all`、`--depends-on-any`

#### `--depends-on-all <IDs>`

多个任务依赖，使用 AND 逻辑。任务等待**所有**指定任务成功完成。

**示例**：
```bash
# 运行三个预处理任务
gbatch python preprocess_part1.py  # 任务 101
gbatch python preprocess_part2.py  # 任务 102
gbatch python preprocess_part3.py  # 任务 103

# 训练等待所有预处理任务
gbatch --depends-on-all 101,102,103 --gpus 2 python train.py

# 使用 @ 语法
gbatch --depends-on-all @,@~1,@~2 --gpus 2 python train.py
```

**行为**：
- 任务等待**所有**依赖达到 `Finished` 状态
- 如果任何依赖失败，此任务默认自动取消
- 接受逗号分隔的任务 ID 或简写
- 参见 [任务依赖](../user-guide/job-dependencies)

**冲突选项**：`--depends-on`、`--depends-on-any`

#### `--depends-on-any <IDs>`

多个任务依赖，使用 OR 逻辑。当**任何一个**指定任务成功完成时启动任务。

**示例**：
```bash
# 并行尝试多个数据源
gbatch python fetch_from_source_a.py  # 任务 201
gbatch python fetch_from_source_b.py  # 任务 202
gbatch python fetch_from_source_c.py  # 任务 203

# 处理首先成功的数据源
gbatch --depends-on-any 201,202,203 python process_data.py

# 使用 @ 语法
gbatch --depends-on-any @,@~1,@~2 python process_data.py
```

**行为**：
- 当**任何一个**依赖达到 `Finished` 状态时任务启动
- 如果所有依赖都失败，此任务默认自动取消
- 接受逗号分隔的任务 ID 或简写
- 适用于回退场景
- 参见 [任务依赖](../user-guide/job-dependencies)

**冲突选项**：`--depends-on`、`--depends-on-all`

#### `--no-auto-cancel`

禁用依赖失败时的自动取消。

**示例**：
```bash
# 如果依赖失败，任务将保持在队列中
gbatch --depends-on 1 --no-auto-cancel python train.py
```

**行为**：
- 默认情况下，当依赖失败时任务会自动取消
- 使用此标志，任务将无限期保持在 `Queued` 状态
- 如果依赖失败，您必须使用 `gcancel` 手动取消
- 适用于 `--depends-on`、`--depends-on-all` 和 `--depends-on-any`

#### `--time <TIME>`, `-t <TIME>`

最大任务运行时间（时间限制）。

**格式**：
- `MM`：仅分钟（例如 `30` = 30 分钟）
- `MM:SS`：分钟和秒（例如 `5:30` = 5 分 30 秒）
- `HH:MM:SS`：小时、分钟、秒（例如 `2:00:00` = 2 小时）

**示例**：
```bash
# 30 分钟
gbatch --time 30 python quick.py

# 2 小时
gbatch --time 2:00:00 python train.py

# 5 分钟 30 秒
gbatch -t 5:30 python test.py
```

**行为**：
- 如果超过限制，任务被终止
- 优雅终止（SIGINT/Ctrl-C）
- 状态变为 `Timeout`
- 参见 [时间限制](../user-guide/time-limits)

#### `--array <SPEC>`

创建任务数组。

**格式**：`START-END`（例如 `1-10`）

**示例**：
```bash
# 创建 10 个任务（任务 1-10）
gbatch --array 1-10 python process.py --task $GFLOW_ARRAY_TASK_ID

# 创建 5 个带 GPU 的任务
gbatch --array 1-5 --gpus 1 python train.py --fold $GFLOW_ARRAY_TASK_ID
```

**行为**：
- 创建多个独立任务
- 每个任务获得唯一的 `$GFLOW_ARRAY_TASK_ID`
- 所有任务共享相同的资源要求
- 适用于参数扫描

**环境变量**：
- `GFLOW_ARRAY_TASK_ID`：任务号（1, 2, 3, ...）
- 对于非数组任务设置为 0

#### `--name <NAME>`

自定义任务名称。

**示例**：
```bash
gbatch --name "my-training-run" python train.py
gbatch --name "experiment-1" my_job.sh
```

**行为**：
- 默认：自动生成名称（例如 "silent-pump-6338"）
- 用作 tmux 会话名称
- 帮助在队列中识别任务
- 必须唯一（或 gflow 附加后缀）

### 全局选项

#### `--config <PATH>`

使用自定义配置文件（隐藏选项）。

**示例**：
```bash
gbatch --config /path/to/custom.toml your_command
```

#### `--help`, `-h`

显示帮助消息。

```bash
$ gbatch --help
<!-- cmdrun gbatch --help -->
```

#### `--version`, `-V`

显示版本信息。

```bash
$ gbatch --version
<!-- cmdrun gbatch --version -->
```

## 脚本指令

使用 `# GFLOW` 注释在脚本中嵌入任务参数。

### 语法

```bash
#!/bin/bash
# GFLOW --option value
# GFLOW --another-option value

# 你的命令在这里
```

### 支持的指令

```bash
# GFLOW --gpus <N>
# GFLOW --time <TIME>
# GFLOW --priority <N>
# GFLOW --conda-env <ENV>
# GFLOW --depends-on <ID>
# GFLOW --depends-on-all <IDs>
# GFLOW --depends-on-any <IDs>
# GFLOW --no-auto-cancel
```

### 示例脚本

```bash
#!/bin/bash
# GFLOW --gpus 1
# GFLOW --time 2:00:00
# GFLOW --priority 20
# GFLOW --conda-env pytorch

echo "Starting training at $(date)"
python train.py --epochs 100
echo "Training complete at $(date)"
```

### 优先级

命令行选项覆盖脚本指令：

```bash
# 脚本有：# GFLOW --time 1:00:00
# CLI 覆盖它：
gbatch --time 2:00:00 my_script.sh  # 使用 2 小时，不是 1 小时
```

## 模板创建

### `new` 子命令

```bash
gbatch new <NAME>
```

创建 `<NAME>.sh` 文件，包含模板结构。

**示例**：
```bash
$ gbatch new my_job
Created template: my_job.sh

$ cat my_job.sh
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

**使用**：
1. 创建模板：`gbatch new my_job`
2. 编辑脚本：`vim my_job.sh`
3. 设置可执行：`chmod +x my_job.sh`
4. 提交：`gbatch my_job.sh`

## 环境变量

gflow 在你的任务中设置这些变量：

| 变量 | 描述 | 示例 |
|------|------|------|
| `CUDA_VISIBLE_DEVICES` | 分配的 GPU ID | `0,1` |
| `GFLOW_ARRAY_TASK_ID` | 数组任务 ID（非数组为 0） | `5` |

**脚本中的使用**：
```bash
#!/bin/bash
echo "GPUs: $CUDA_VISIBLE_DEVICES"
echo "Task: $GFLOW_ARRAY_TASK_ID"
python train.py
```

## 输出

### 提交成功

```bash
$ gbatch python train.py
Submitted batch job 42 (silent-pump-6338)
```

**格式**：`Submitted batch job <ID> (<NAME>)`

### 任务日志

输出捕获到：`~/.local/share/gflow/logs/<JOBID>.log`

**查看日志**：
```bash
# 查看已完成的任务
cat ~/.local/share/gflow/logs/42.log

# 跟踪运行中的任务
tail -f ~/.local/share/gflow/logs/42.log
```

## 示例

### 基本提交

```bash
# 直接命令
gbatch echo 'Hello, gflow!'

# 脚本文件
gbatch my_script.sh

# 带选项
gbatch --gpus 1 --time 2:00:00 python train.py
```

### 资源分配

```bash
# GPU 任务
gbatch --gpus 2 python multi_gpu_train.py

# 带时间限制的 CPU 任务
gbatch --time 30 python preprocess.py

# Conda 环境（显式）
gbatch --conda-env myenv python script.py

# Conda 环境（从当前活跃环境自动检测）
conda activate myenv
gbatch python script.py  # 自动使用 'myenv'
```

### 任务依赖

```bash
# 顺序管道
gbatch --time 30 python prep.py

gbatch --depends-on @ --gpus 1 python train.py

gbatch --depends-on @ python eval.py
```

使用 `@` 引用最近提交的任务。

### 任务数组

```bash
# 并行处理 10 个任务
gbatch --array 1-10 python process.py --id $GFLOW_ARRAY_TASK_ID

# GPU 扫描
gbatch --array 1-5 --gpus 1 --time 4:00:00 \
       python train.py --lr $(echo "0.001 0.01 0.1 0.5 1.0" | cut -d" " -f$GFLOW_ARRAY_TASK_ID)
```

### 优先级和调度

```bash
# 紧急任务
gbatch --priority 100 --gpus 1 python urgent.py

# 后台任务
gbatch --priority 1 python background.py

# 命名任务
gbatch --name "exp-baseline" python train_baseline.py
```

### 脚本模板

```bash
# 创建模板
gbatch new experiment

# 编辑它
vim experiment.sh

# 提交它
gbatch experiment.sh
```

## 验证和错误

### 常见错误

```bash
# 同时指定脚本和命令
Error: Cannot specify both script and direct command

# 脚本未找到
Error: Script file not found: missing.sh

# 无效的依赖
Error: Dependency job 999 not found

# 循环依赖
Error: Circular dependency detected

# 无效的时间格式
Error: Invalid time format. Use HH:MM:SS, MM:SS, or MM

# Conda 环境未找到
Error: Conda environment 'invalid_env' not found
```

### 验证检查

在提交时，gbatch 验证：
- ✅ 脚本存在（如果使用脚本模式）
- ✅ 依赖任务存在
- ✅ 无循环依赖
- ✅ 有效的时间格式
- ✅ 不同时指定脚本和直接命令
- ✅ GPU 数量合理

## 集成示例

### Shell 脚本：批量提交

```bash
#!/bin/bash
# submit_experiments.sh - 提交多个实验

for lr in 0.001 0.01 0.1; do
    for bs in 32 64 128; do
        gbatch --gpus 1 --time 4:00:00 \
               --name "lr${lr}_bs${bs}" \
               python train.py --lr $lr --batch-size $bs
    done
done
```

### Shell 脚本：管道提交

```bash
#!/bin/bash
# submit_pipeline.sh - 提交数据处理管道

set -e

echo "Submitting pipeline..."

# 阶段 1：下载
gbatch --time 1:00:00 --name "download" python download.py
echo "Job download submitted"

# 阶段 2：处理（依赖下载）
gbatch --depends-on @ --time 2:00:00 --name "process" python process.py
echo "Job process submitted"

# 阶段 3：训练（依赖处理）
gbatch --depends-on @ --gpus 1 --time 8:00:00 --name "train" python train.py
echo "Job train submitted"

echo "Pipeline submitted! Monitor with: watch gqueue -t"
```

### Python 脚本：任务提交

```python
#!/usr/bin/env python3
# submit_jobs.py - 从 Python 提交任务

import subprocess
import re

def submit_job(command, **kwargs):
    """提交任务并返回其 ID。"""
    cmd = ['gbatch']

    # 首先添加选项
    if 'gpus' in kwargs:
        cmd += ['--gpus', str(kwargs['gpus'])]
    if 'time' in kwargs:
        cmd += ['--time', kwargs['time']]
    if 'priority' in kwargs:
        cmd += ['--priority', str(kwargs['priority'])]
    if 'depends_on' in kwargs:
        cmd += ['--depends-on', str(kwargs['depends_on'])]
    if 'name' in kwargs:
        cmd += ['--name', kwargs['name']]

    # 在末尾添加命令
    cmd += command.split() if isinstance(command, str) else command

    result = subprocess.run(cmd, capture_output=True, text=True)
    match = re.search(r'job (\d+)', result.stdout)
    return int(match.group(1)) if match else None

# 提交管道
prep_id = submit_job('python preprocess.py', time='30', name='prep')
train_id = submit_job('python train.py', gpus=1, time='4:00:00',
                       depends_on=prep_id, name='train')
eval_id = submit_job('python evaluate.py', time='10',
                      depends_on=train_id, name='eval')

print(f"Pipeline: {prep_id} -> {train_id} -> {eval_id}")
```

## 最佳实践

1. **始终为生产任务设置时间限制**
   ```bash
   gbatch --time 2:00:00 your_command
   ```

2. **使用有意义的名称便于跟踪**
   ```bash
   gbatch --name "exp-baseline-lr0.01" your_command
   ```

3. **提交前在本地测试脚本**
   ```bash
   bash my_job.sh  # 先测试
   gbatch my_job.sh  # 然后提交
   ```

4. **仅请求需要的 GPU**
   ```bash
   gbatch --gpus 1 "..."  # 不是 --gpus 4（如果只需要 1 个）
   ```

5. **对工作流使用依赖**
   ```bash
   gbatch python prep.py
   gbatch --depends-on @ python train.py
   ```

6. **对并行任务使用任务数组**
   ```bash
   gbatch --array 1-10 python process.py --id $GFLOW_ARRAY_TASK_ID
   ```

7. **在脚本中添加错误处理**
   ```bash
   #!/bin/bash
   set -e  # 出错时退出
   ```

8. **在任务中记录重要信息**
   ```bash
   echo "Job started: $(date)"
   echo "GPUs: $CUDA_VISIBLE_DEVICES"
   ```

## 参见

- [gqueue](./gqueue-reference) - 任务队列参考
- [gcancel](./gcancel-reference) - 任务取消参考
- [ginfo](./ginfo-reference) - 调度器检查参考
- [任务提交](../user-guide/job-submission) - 详细提交指南
- [任务依赖](../user-guide/job-dependencies) - 工作流管理
- [GPU 管理](../user-guide/gpu-management) - GPU 分配指南
- [时间限制](../user-guide/time-limits) - 时间限制文档
- [快速参考](./quick-reference) - 命令速查表
