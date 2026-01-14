# 快速入门

本指南将帮助您在 5 分钟内开始使用 gflow。

## 启动调度器

首先，启动 gflow 守护进程：

```shell
$ gflowd up
```

此命令在 tmux 会话中启动守护进程。如果您想在后台运行它，可以使用：
```shell
$ gflowd up --detach
```

在其自己的终端或 tmux 会话中运行此命令并保持运行。您可以使用以下命令确认它已成功启动：
```shell
$ gflowd status
```

示例输出：
```
Status: Running
The gflowd daemon is running in tmux session 'gflow_server'.
```

从另一个终端验证它是否可访问：
```shell
$ ginfo
```

输出将显示调度器状态和 GPU 信息。

## 您的第一个任务

让我们提交一个简单的任务：

```shell
$ gbatch echo 'Hello from gflow!'
```

这将提交一个任务，将"Hello from gflow!"打印到日志中。

## 检查任务状态

查看任务队列：

```shell
$ gqueue
```

示例输出：
```
JOBID   NAME            ST   TIME       NODES   NODELIST(REASON)
1       gflow-job-1     CD   00:00:00   0       -
```

任务状态：
- `PD`（Queued）- 等待运行
- `R`（Running）- 当前正在执行
- `CD`（Finished）- 成功完成
- `F`（Failed）- 失败并出错
- `CA`（Cancelled）- 手动取消
- `TO`（Timeout）- 超过时间限制

## 查看任务输出

任务输出会自动记录：

```shell
$ sleep 6
$ gjob log 1
```

## 使用选项提交任务

### 请求 GPU 的任务

```shell
gbatch --gpus 1 nvidia-smi
```

### 带时间限制的任务

```shell
# 30 分钟限制
gbatch --time 30 python train.py

# 2 小时限制
gbatch --time 2:00:00 python long_train.py
```

### 带优先级的任务

```shell
# 更高优先级（优先运行）
gbatch --priority 100 python urgent_task.py

# 较低优先级（默认为 10）
gbatch --priority 5 python background_task.py
```

### 任务脚本

创建文件 `my_job.sh`：
```shell
#!/bin/shell
# GFLOW --gpus 1
# GFLOW --time 1:00:00
# GFLOW --priority 20

echo "任务开始于 $(date)"
python train.py --epochs 10
echo "任务完成于 $(date)"
```

使其可执行并提交：
```shell
chmod +x my_job.sh
gbatch my_job.sh
```

## 任务依赖

使用 @ 语法按顺序运行任务：

```shell
# 任务 1：预处理
gbatch --name "prep" python preprocess.py

# 任务 2：训练（依赖于任务 1）
gbatch --depends-on @ --name "train" python train.py

# 任务 3：评估（依赖于任务 2）
gbatch --depends-on @ --name "eval" python evaluate.py
```

`@` 符号始终引用最近提交的任务，使链接依赖关系变得容易。

查看依赖树：
```shell
gqueue -t
```

## 监控任务

### 实时监控队列

```shell
watch -n 2 gqueue
```

### 按状态过滤

```shell
# 仅显示运行中的任务
gqueue -s Running

# 显示运行中和排队中的任务
gqueue -s Running,Queued
```

### 自定义输出格式

```shell
$ gqueue -f JOBID,NAME,ST,TIME,TIMELIMIT
```

示例输出：
```
JOBID   NAME            ST   TIME       TIMELIMIT
1       gflow-job-1     CD   00:00:05   00:01:00
2       gflow-job-2     R    00:00:10   UNLIMITED
```

### 查看特定任务

```shell
# 单个任务
gqueue -j 5

# 多个任务
gqueue -j 5,6,7
```

## 取消任务

取消任务：

```shell
gcancel 5
```

输出：
```
Job 5 cancelled.
```

## 附加到运行中的任务

每个任务都在 tmux 会话中运行。您可以附加以查看实时输出：

```shell
# 从 gqueue 获取任务的会话名称
gqueue -f JOBID,NAME

# 附加到会话
gjob attach -t <job_id>

# 分离而不停止任务
# 按：Ctrl+B 然后 D
```

## 停止调度器

完成后：

```shell
gflowd down
```

这将停止守护进程，保存状态并删除 tmux 会话。

## 示例工作流

这是一个完整的示例工作流：

```shell
# 1. 启动调度器
gflowd up

# 2. 提交预处理任务
gbatch --time 10 --name prep python preprocess.py

# 3. 提交依赖于预处理的训练任务
gbatch --time 2:00:00 --gpus 1 --depends-on @ --name train_lr001 python train.py --lr 0.001

gbatch --time 2:00:00 --gpus 1 --depends-on @~1 --name train_lr01 python train.py --lr 0.01

# 注意：两个训练任务都依赖于 @~1（prep 任务），跳过彼此

# 4. 监控任务
watch gqueue -t

# 5. 完成后检查日志
gjob log 1
gjob log 2
gjob log 3

# 6. 停止调度器
gflowd down
```

**@ 语法说明**：
- `@` - 最近提交的任务
- `@~1` - 第二个最近提交的任务
- `@~2` - 第三个最近提交的任务

这使得创建复杂的工作流变得容易，无需手动跟踪任务 ID！

## 常见模式

### 并行任务（数组）

运行多个类似的任务：

```shell
gbatch --array 1-10 --time 30 \
       python process.py --task $GFLOW_ARRAY_TASK_ID
```

这将创建 10 个任务，每个任务的 `$GFLOW_ARRAY_TASK_ID` 设置为 1、2、...、10。

### GPU 扫描

在不同的 GPU 上测试不同的超参数：

```shell
# 每个任务获得 1 个 GPU
gbatch --gpus 1 --time 4:00:00 python train.py --lr 0.001
gbatch --gpus 1 --time 4:00:00 python train.py --lr 0.01
gbatch --gpus 1 --time 4:00:00 python train.py --lr 0.1
```

### Conda 环境

使用特定的 conda 环境：

```shell
gbatch --conda-env myenv python script.py
```

## 新手提示

1. **始终为生产任务设置时间限制**：
   ```shell
   gbatch --time 2:00:00 your_command
   ```

2. **使用 `watch gqueue`** 实时监控任务

3. **任务失败时检查日志**：
   ```shell
   cat ~/.local/share/gflow/logs/<job_id>.log
   ```

4. **首先使用短时间限制测试脚本**：
   ```shell
   gbatch --time 1 shell test.sh
   ```

5. **为工作流使用任务依赖**：
   ```shell
   gbatch --depends-on <prev_job_id> your_command
   ```

## 下一步

现在您已经熟悉了基础知识，请探索：

- [任务提交](../user-guide/job-submission) - 详细的任务选项
- [时间限制](../user-guide/time-limits) - 管理任务超时
- [任务依赖](../user-guide/job-dependencies) - 复杂的工作流
- [GPU 管理](../user-guide/gpu-management) - GPU 分配
- [快速参考](../reference/quick-reference) - 命令速查表

---

**上一页**：[安装](./installation) | **下一页**：[任务提交](../user-guide/job-submission)
