# GPU 管理

gflow 会检测 NVIDIA GPU（通过 NVML），并通过设置 `CUDA_VISIBLE_DEVICES` 将 GPU 分配给任务。

## 快速开始

```bash
# 启动守护进程（如未运行）
gflowd up

# 查看可用性与当前分配
ginfo

# 提交 GPU 任务
gbatch --gpus 1 python train.py

# 跟踪任务与分配
gqueue -s Running,Queued -f JOBID,NAME,ST,NODES,NODELIST(REASON)
```

## 查看 GPU

```bash
ginfo
```

示例输出：
```
PARTITION  GPUS  NODES  STATE      JOB(REASON)
gpu        1     1      idle
gpu        1     0      allocated  5 (train-resnet)
```

- `NODES` 为物理 GPU 索引。
- 若 GPU 被占用但不是由 gflow 分配，可能会以“原因”的形式显示（如可获取）。

非 gflow 占用：
- 如果 NVML 检测到某张 GPU 上有运行中的计算进程，gflow 会将其视为不可用（常显示为 `Unmanaged`），不会去分配这张卡。
- gflow 不会抢占/终止非 gflow 进程；任务只会等待 GPU 变为空闲后再运行。

如需查看每张 GPU 是否被限制（allowed vs restricted）：

```bash
gctl show-gpus
```

### 依赖条件

- NVIDIA GPU + 驱动
- NVML 库可用（`libnvidia-ml.so`）

快速检查：

```bash
nvidia-smi
gflowd up
ginfo
```

无 GPU 的机器也可以正常使用 gflow；仅 GPU 分配不可用。

## 请求 GPU

```bash
gbatch --gpus 1 python train.py
gbatch --gpus 2 python multi_gpu_train.py
```

任务启动时，gflow 会分配**物理 GPU 索引**并导出到 `CUDA_VISIBLE_DEVICES`（多数框架会从 `0` 开始重新编号）。

查看某个任务实际分到的 GPU：

```bash
gqueue -s Running -f JOBID,NAME,ST,NODES,NODELIST(REASON)
gjob show <job_id>
```

### GPU 可见性

```bash
#!/bin/bash
# GFLOW --gpus 2

echo "CUDA_VISIBLE_DEVICES=$CUDA_VISIBLE_DEVICES"
python train.py
```

## 限制 gflow 可用 GPU

限制调度器允许分配的物理 GPU（只影响新的分配）：

```bash
gctl set-gpus 0,2
gctl show-gpus

# 或使用守护进程 CLI 参数（覆盖配置文件）
gflowd restart --gpus 0-3
```

另见：[配置 -> GPU 选择](./configuration#gpu-selection)。

## 故障排除

### 任务拿不到 GPU

```bash
ginfo
gqueue -j <job_id> -f JOBID,ST,NODES,NODELIST(REASON)
gctl show-gpus
```

### 任务看到错误的 GPU

```bash
echo "CUDA_VISIBLE_DEVICES=$CUDA_VISIBLE_DEVICES"
gqueue -f JOBID,NODELIST(REASON)
```

### 显存不足

```bash
nvidia-smi --query-gpu=memory.free,memory.used --format=csv
```

## 另见

- [任务提交](./job-submission) - 完整的任务提交指南
- [任务依赖](./job-dependencies) - 工作流管理
- [时间限制](./time-limits) - 任务超时管理
- [快速参考](../reference/quick-reference) - 命令速查表
