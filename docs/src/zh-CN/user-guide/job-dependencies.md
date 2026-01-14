# 任务依赖

本指南涵盖了如何在 gflow 中使用任务依赖创建复杂工作流。

## 概述

任务依赖允许您创建工作流，其中任务等待其他任务完成后才开始。这对以下情况至关重要：
- 多阶段管道（预处理 → 训练 → 评估）
- 具有数据依赖的顺序工作流
- 基于前一个结果的条件执行
- 资源优化（在阶段之间释放 GPU）

## 基本用法

### 简单依赖

提交依赖于另一个任务的任务：

```bash
# 任务 1：预处理
$ gbatch --name "prep" python preprocess.py
Submitted batch job 1 (prep)

# 任务 2：训练（等待任务 1）
$ gbatch --depends-on 1 --name "train" python train.py
Submitted batch job 2 (train)
```

**工作原理**：
- 任务 2 仅在任务 1 成功完成后（状态：`Finished`）才开始
- 如果任务 1 失败，任务 2 将无限期保持在 `Queued` 状态
- 如果任务 1 失败，您必须手动取消任务 2

**@ 语法糖**：您可以引用最近的提交而无需复制 ID：
- `--depends-on @` - 最近的提交（最后提交的任务）
- `--depends-on @~1` - 倒数第二个提交
- `--depends-on @~2` - 倒数第三个提交
- 等等...

这使创建管道变得更加简单！

### 检查依赖

查看依赖关系：

```bash
$ gqueue -t
JOBID    NAME      ST    TIME         TIMELIMIT
1        prep      CD    00:02:15     UNLIMITED
└─ 2     train     R     00:05:30     04:00:00
   └─ 3  eval      PD    00:00:00     00:10:00
```

树视图（`-t`）以 ASCII 艺术显示依赖层次结构。

## 创建工作流

### 线性管道

使用 @ 语法按顺序执行任务：

```bash
# 阶段 1：数据收集
gbatch --time 10 python collect_data.py

# 阶段 2：数据预处理（依赖于阶段 1）
gbatch --time 30 --depends-on @ python preprocess.py

# 阶段 3：训练（依赖于阶段 2）
gbatch --time 4:00:00 --gpus 1 --depends-on @ python train.py

# 阶段 4：评估（依赖于阶段 3）
gbatch --time 10 --depends-on @ python evaluate.py
```

**监控管道**：
```bash
watch -n 5 gqueue -t
```

**工作原理**：每个 `--depends-on @` 引用紧接着提交的任务，创建一个清晰的顺序管道。

### 并行处理与合并

多个任务汇入一个：

```bash
# 并行数据处理任务
gbatch --time 30 python process_part1.py  # 任务 10
gbatch --time 30 python process_part2.py  # 任务 11
gbatch --time 30 python process_part3.py  # 任务 12

# 合并结果（等待所有并行任务）
gbatch --depends-on-all 10,11,12 python merge_results.py
```

**使用 @ 语法**：
```bash
gbatch --time 30 python process_part1.py
gbatch --time 30 python process_part2.py
gbatch --time 30 python process_part3.py

# 使用 @ 语法引用最近的三个任务
gbatch --depends-on-all @,@~1,@~2 python merge_results.py
```

### 分支工作流

一个任务触发多个下游任务：

```bash
# 主处理
gbatch --time 1:00:00 python main_process.py

# 多个分析任务（都依赖于主任务）
gbatch --depends-on @ --time 30 python analysis_a.py
gbatch --depends-on @~1 --time 30 python analysis_b.py
gbatch --depends-on @~2 --time 30 python analysis_c.py
```

**说明**：
- 第一个分析依赖于 `@`（main_process 任务）
- 第二个分析依赖于 `@~1`（跳过 analysis_a，回到 main_process）
- 第三个分析依赖于 `@~2`（跳过 analysis_a 和 analysis_b，回到 main_process）

## 依赖状态和行为

### 依赖何时开始

具有依赖的任务从 `Queued` 转换到 `Running` 时：
1. 依赖任务达到 `Finished` 状态
2. 所需资源（GPU 等）可用

### 失败的依赖

如果依赖任务失败：
- 依赖任务会**自动取消**（默认行为）
- 您可以使用 `--no-auto-cancel` 禁用自动取消，使任务保持在 `Queued` 状态

**示例（默认自动取消）**：
```bash
# 任务 1 失败
$ gqueue
JOBID    NAME      ST    TIME
1        prep      F     00:01:23
2        train     CA    00:00:00

# 任务 2 已自动取消
```

**示例（禁用自动取消）**：
```bash
# 提交时禁用自动取消
gbatch --depends-on 1 --no-auto-cancel python train.py

# 如果任务 1 失败，任务 2 将保持在队列中
$ gqueue
JOBID    NAME      ST    TIME
1        prep      F     00:01:23
2        train     PD    00:00:00

# 任务 2 永远不会运行 - 必须手动取消它
$ gcancel 2
```

### 超时依赖

如果依赖任务超时：
- 状态更改为 `Timeout`（TO）
- 处理方式与 `Failed` 相同
- 依赖任务保持在队列中

### 已取消的依赖

如果您取消具有依赖的任务：
- 任务被取消
- 依赖任务保持在队列中（不会启动）
- 取消前使用 `gcancel --dry-run` 查看影响

**检查取消影响**：
```bash
$ gcancel --dry-run 1
Would cancel job 1 (prep)
Warning: The following jobs depend on job 1:
  - Job 2 (train)
  - Job 3 (eval)
These jobs will never start if job 1 is cancelled.
```

## 依赖可视化

### 树视图

树视图清晰地显示任务依赖：

```bash
$ gqueue -t
JOBID    NAME           ST    TIME         TIMELIMIT
1        data-prep      CD    00:05:23     01:00:00
├─ 2     train-model-a  R     00:15:45     04:00:00
│  └─ 4  eval-a         PD    00:00:00     00:10:00
└─ 3     train-model-b  R     00:15:50     04:00:00
   └─ 5  eval-b         PD    00:00:00     00:10:00
```

**图例**：
- `├─`：分支连接
- `└─`：最后一个子连接
- `│`：继续线

### 循环依赖检测

gflow 检测并防止循环依赖：

```bash
# 这将失败
$ gbatch --depends-on 2 python a.py
Submitted batch job 1

$ gbatch --depends-on 1 python b.py
Error: Circular dependency detected: Job 2 depends on Job 1, which depends on Job 2
```

**保护**：
- 验证在提交时进行
- 防止任务队列中的死锁
- 确保所有依赖最终可以解决

## 高级模式

### 检查点管道

从失败点恢复：

```bash
#!/bin/bash
# pipeline.sh - 从检查点恢复

set -e

if [ ! -f "data.pkl" ]; then
    echo "Stage 1: Preprocessing"
    python preprocess.py
fi

if [ ! -f "model.pth" ]; then
    echo "Stage 2: Training"
    python train.py
fi

echo "Stage 3: Evaluation"
python evaluate.py
```

提交：
```bash
gbatch --gpus 1 --time 8:00:00 pipeline.sh
```

### 条件依赖脚本

创建基于前一个结果提交任务的脚本：

```bash
#!/bin/bash
# conditional_submit.sh

# 等待任务 1 完成
while [ "$(gqueue -j 1 -f ST | tail -n 1)" = "R" ]; do
    sleep 5
done

# 检查是否成功
STATUS=$(gqueue -j 1 -f ST | tail -n 1)

if [ "$STATUS" = "CD" ]; then
    echo "Job 1 succeeded, submitting next job"
    gbatch python next_step.py
else
    echo "Job 1 failed with status: $STATUS"
    exit 1
fi
```

### 带依赖的数组任务

创建依赖于预处理任务的任务数组：

```bash
# 预处理
gbatch --time 30 python preprocess.py

# 数组训练任务（都依赖于预处理）
for i in {1..5}; do
    gbatch --depends-on @ --gpus 1 --time 2:00:00 \
           python train.py --fold $i
done
```

**注意**：所有数组任务使用 `--depends-on @`，它引用预处理任务，因为在循环开始前它总是最近的非数组提交。

### 资源高效的管道

在阶段之间释放 GPU：

```bash
# 阶段 1：仅 CPU 预处理
gbatch --time 30 python preprocess.py

# 阶段 2：GPU 训练
gbatch --depends-on @ --gpus 2 --time 4:00:00 python train.py

# 阶段 3：仅 CPU 评估
gbatch --depends-on @ --time 10 python evaluate.py
```

**优势**：GPU 仅在需要时分配，最大化资源利用率。

## 监控依赖

### 检查依赖状态

```bash
# 查看特定任务及其依赖
gqueue -j 1,2,3 -f JOBID,NAME,ST,TIME

# 以树格式查看所有任务
gqueue -t

# 按状态过滤并查看依赖
gqueue -s Queued,Running -t
```

### 监控管道进度

```bash
# 实时监控
watch -n 2 'gqueue -t'

# 仅显示活跃任务
watch -n 2 'gqueue -s Running,Queued -t'
```

### 识别被阻止的任务

查找等待依赖的任务：

```bash
# 显示带依赖信息的队列任务
gqueue -s Queued -t

# 检查任务为何在队列中
gqueue -j 5 -f JOBID,NAME,ST
gqueue -t | grep -A5 "^5"
```

## 依赖验证

### 提交时验证

`gbatch` 在提交时验证依赖：

✅ **有效提交**：
- 依赖任务存在
- 没有循环依赖
- 依赖不是任务本身

❌ **无效提交**：
- 依赖任务不存在：`Error: Dependency job 999 not found`
- 循环依赖：`Error: Circular dependency detected`
- 自依赖：`Error: Job cannot depend on itself`

### 运行时行为

执行期间：
- 调度器每 5 秒检查一次依赖
- 当依赖为 `Finished` 且资源可用时任务启动
- 失败/超时依赖永远不会触发依赖任务

## 实际示例

### 示例 1：ML 训练管道

```bash
# 使用 @ 语法的完整 ML 管道
gbatch --time 20 python prepare_dataset.py

gbatch --depends-on @ --gpus 1 --time 8:00:00 \
       python train.py --output model.pth

gbatch --depends-on @ --time 15 \
       python evaluate.py --model model.pth

gbatch --depends-on @ --time 5 python generate_report.py
```

### 示例 2：数据处理管道

```bash
#!/bin/bash
# 提交数据处理管道

echo "Submitting data processing pipeline..."

# 下载数据
gbatch --time 1:00:00 --name "download" python download_data.py

# 验证数据
gbatch --depends-on @ --time 30 --name "validate" python validate_data.py

# 转换数据
gbatch --depends-on @ --time 45 --name "transform" python transform_data.py

# 上传结果
gbatch --depends-on @ --time 30 --name "upload" python upload_results.py

echo "Pipeline submitted. Monitor with: watch gqueue -t"
```

### 示例 3：带评估的超参数扫描

```bash
# 训练多个模型
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.001 --output model_0.001.pth  # 任务 10
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.01 --output model_0.01.pth    # 任务 11
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.1 --output model_0.1.pth      # 任务 12

# 等待所有模型，然后评估
gbatch --depends-on-all 10,11,12 --time 30 python compare_models.py --models model_*.pth
```

**使用 @ 语法**：
```bash
# 训练多个模型
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.001 --output model_0.001.pth
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.01 --output model_0.01.pth
gbatch --gpus 1 --time 2:00:00 python train.py --lr 0.1 --output model_0.1.pth

# 使用 @ 语法等待所有三个训练任务
gbatch --depends-on-all @,@~1,@~2 --time 30 python compare_models.py --models model_*.pth
```

## 故障排除

### 问题：依赖任务未启动

**可能原因**：
1. 依赖任务未完成：
   ```bash
   gqueue -t
   ```

2. 依赖任务失败：
   ```bash
   gqueue -j <dep_id> -f JOBID,ST
   ```

3. 没有可用资源（GPU）：
   ```bash
   ginfo
   gqueue -s Running -f NODES,NODELIST
   ```

### 问题：想要取消具有依赖的任务

**解决方案**：先使用 dry-run 查看影响：
```bash
# 查看会发生什么
gcancel --dry-run <job_id>

# 如果可接受则取消
gcancel <job_id>

# 如果需要也取消依赖任务
gcancel <job_id>
gcancel <dependent_job_id>
```

### 问题：循环依赖错误

**解决方案**：检查您的依赖链：
```bash
# 检查任务序列
gqueue -j <job_ids> -t

# 重新构造以消除循环
```

### 问题：丢失了依赖的跟踪

**解决方案**：使用树视图：
```bash
# 显示所有任务关系
gqueue -a -t

# 关注特定任务
gqueue -j 1,2,3,4,5 -t
```

## 最佳实践

1. **规划工作流** 提交任务前
2. **使用有意义的名称** 用于管道中的任务（`--name` 标志）
3. **使用 @ 语法** 用于更简单的依赖链
4. **为每个阶段设置适当的时间限制**
5. **使用 `watch gqueue -t` 监控管道**
6. **通过检查依赖状态处理失败**
7. **取消具有依赖的任务前使用 dry-run**
8. **在提交脚本中记录管道**
9. **提交长管道前先测试小规模**
10. **依赖失败时检查日志**

## 多任务依赖

gflow 支持高级依赖模式，其中一个任务可以依赖多个父任务，并使用不同的逻辑模式。

### AND 逻辑（所有依赖必须成功）

当任务应该等待**所有**父任务成功完成时，使用 `--depends-on-all`：

```bash
# 并行运行三个预处理任务
gbatch --time 30 python preprocess_part1.py  # 任务 101
gbatch --time 30 python preprocess_part2.py  # 任务 102
gbatch --time 30 python preprocess_part3.py  # 任务 103

# 训练等待所有预处理任务完成
gbatch --depends-on-all 101,102,103 --gpus 2 --time 4:00:00 python train.py
```

**使用 @ 语法**：
```bash
gbatch python preprocess_part1.py  # 任务 101
gbatch python preprocess_part2.py  # 任务 102
gbatch python preprocess_part3.py  # 任务 103

# 使用 @ 语法引用最近的任务
gbatch --depends-on-all @,@~1,@~2 --gpus 2 python train.py
```

### OR 逻辑（任一依赖必须成功）

当任务应该在**任何一个**父任务成功完成后立即启动时，使用 `--depends-on-any`：

```bash
# 并行尝试多个数据源
gbatch --time 10 python fetch_from_source_a.py  # 任务 201
gbatch --time 10 python fetch_from_source_b.py  # 任务 202
gbatch --time 10 python fetch_from_source_c.py  # 任务 203

# 处理首先成功的数据源的数据
gbatch --depends-on-any 201,202,203 python process_data.py
```

**使用场景**：回退场景，其中并行尝试多种方法，您希望使用第一个成功的结果继续。

### 自动取消

默认情况下，当父任务失败时，所有依赖任务会**自动取消**：

```bash
gbatch python preprocess.py  # 任务 301 - 失败

# 当 301 失败时，任务 302 将自动取消
gbatch --depends-on 301 python train.py  # 任务 302
```

**禁用自动取消**，如果您希望依赖任务保持在队列中：

```bash
gbatch --depends-on 301 --no-auto-cancel python train.py
```

**何时发生自动取消**：
- 父任务失败（状态：`Failed`）
- 父任务被取消（状态：`Cancelled`）
- 父任务超时（状态：`Timeout`）

**级联取消**：如果任务 A 依赖于 B，B 依赖于 C，当 C 失败时，B 和 A 都会自动取消。

### 循环依赖检测

gflow 在提交时自动检测并防止循环依赖：

```bash
gbatch python job_a.py  # 任务 1
gbatch --depends-on 1 python job_b.py  # 任务 2

# 这将被拒绝并显示错误
gbatch --depends-on 2 python job_c.py --depends-on-all 1,2  # 会创建循环
```

**错误消息**：
```
Circular dependency detected: Job 3 depends on Job 2, which has a path back to Job 3
```

### 复杂工作流示例

结合 AND 和 OR 逻辑创建复杂工作流：

```bash
# 阶段 1：尝试多种数据收集方法
gbatch --time 30 python collect_method_a.py  # 任务 1
gbatch --time 30 python collect_method_b.py  # 任务 2

# 阶段 2：处理首先成功的收集
gbatch --depends-on-any 1,2 --time 1:00:00 python process_data.py  # 任务 3

# 阶段 3：并行运行多个预处理任务
gbatch --depends-on 3 --time 30 python preprocess_features.py  # 任务 4
gbatch --depends-on 3 --time 30 python preprocess_labels.py    # 任务 5

# 阶段 4：训练等待两个预处理任务
gbatch --depends-on-all 4,5 --gpus 2 --time 8:00:00 python train.py  # 任务 6

# 阶段 5：评估依赖于训练
gbatch --depends-on 6 --time 30 python evaluate.py  # 任务 7
```

**使用树视图可视化**：
```bash
$ gqueue -t
JOBID    NAME                ST    TIME         TIMELIMIT
1        collect_method_a    CD    00:15:30     00:30:00
2        collect_method_b    F     00:10:00     00:30:00
└─ 3     process_data        CD    00:45:00     01:00:00
   ├─ 4  preprocess_features CD    00:20:00     00:30:00
   └─ 5  preprocess_labels   CD    00:18:00     00:30:00
      └─ 6  train            R     02:30:00     08:00:00
         └─ 7  evaluate      PD    00:00:00     00:30:00
```

## 限制

**剩余限制**：
- 无依赖特定任务状态（例如"当任务 X 失败时启动"）
- 无任务组或批依赖（除了 max_concurrent）

**已移除的限制**（现已支持）：
- ~~每个任务仅一个依赖~~ → 现在支持使用 `--depends-on-all` 和 `--depends-on-any` 的多个依赖
- ~~无自动取消~~ → 现在默认自动取消（可以使用 `--no-auto-cancel` 禁用）

**解决方法**：
- 对于基于状态的依赖，使用检查任务状态的条件脚本
- 如果需要，对于非常复杂的 DAG 使用外部工作流管理器

## 另见

- [任务提交](./job-submission) - 完整的任务提交指南
- [时间限制](./time-limits) - 管理任务超时
- [快速参考](../reference/quick-reference) - 命令速查表
- [快速开始](../getting-started/quick-start) - 基本使用示例
