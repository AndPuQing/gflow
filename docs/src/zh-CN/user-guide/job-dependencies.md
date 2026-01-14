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
gbatch --time 30 python process_part1.py
gbatch --time 30 python process_part2.py
gbatch --time 30 python process_part3.py

# 合并结果（等待最后一个并行任务）
gbatch --depends-on @ python merge_results.py
```

**当前限制**：gflow 目前每个任务仅支持一个依赖。上面的示例显示 `merge_results.py` 依赖于最后一个并行任务（`process_part3.py`）。对于真正的多父依赖（等待所有并行任务），您需要中间协调任务或在检查所有并行任务完成后提交合并。

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
- 依赖任务保持在 `Queued` 状态
- 它将**永远不会**自动启动
- 您必须使用 `gcancel` 手动取消它

**示例**：
```bash
# 任务 1 失败
$ gqueue
JOBID    NAME      ST    TIME
1        prep      F     00:01:23
2        train     PD    00:00:00

# 任务 2 永远不会运行 - 必须取消它
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
for lr in 0.001 0.01 0.1; do
    gbatch --gpus 1 --time 2:00:00 \
           python train.py --lr $lr --output model_$lr.pth
done

# 等待所有模型，然后评估
# （依赖于最后训练的模型）
gbatch --depends-on @ --time 30 \
       python compare_models.py --models model_*.pth
```

**注意**：对于真正的多依赖支持（等待所有模型），您需要：
- 使用在提交前检查任务状态的脚本
- 在所有训练完成后手动提交比较任务

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

## 限制

**当前限制**：
- 每个任务仅一个依赖（无多父依赖）
- 父任务失败时无自动取消依赖
- 无依赖特定任务状态（例如"当任务 X 失败时启动"）
- 无任务组或批依赖

**解决方法**：
- 对于多个依赖，使用中间协调任务
- 监控任务状态并使用脚本有条件地提交
- 对于复杂 DAG，如果需要使用外部工作流管理器

## 另见

- [任务提交](./job-submission) - 完整的任务提交指南
- [时间限制](./time-limits) - 管理任务超时
- [快速参考](../reference/quick-reference) - 命令速查表
- [快速开始](../getting-started/quick-start) - 基本使用示例
