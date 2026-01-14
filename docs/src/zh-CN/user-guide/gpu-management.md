# GPU 管理

本指南涵盖了 gflow 如何管理 GPU 资源，从检测到分配和监控。

## 概述

gflow 通过 NVML 库为 NVIDIA GPU 提供自动检测、分配和管理。它确保多个任务之间的 GPU 利用率高效，同时防止资源冲突。

## GPU 检测

### 检查可用 GPU

查看系统 GPU 信息：

```bash
$ ginfo
```

示例输出：
```
Scheduler Status: Running
Total GPUs: 2
Available GPUs: 1

GPU 0: NVIDIA GeForce RTX 3090
  UUID: GPU-xxxxx...
  Status: In use by job 5

GPU 1: NVIDIA GeForce RTX 3090
  UUID: GPU-yyyyy...
  Status: Available
```

**显示的信息**：
- 系统中的 GPU 总数
- 当前可用（未使用）的 GPU 数量
- 每个设备的 GPU 型号和 UUID
- 当前分配状态（可用或被哪个任务使用）
- **增强显示**：按任务组织显示 GPU 分配，便于查看哪些任务使用哪些 GPU

### 需求

**系统需求**：
- NVIDIA GPU
- 已安装 NVIDIA 驱动程序
- NVML 库可用（`libnvidia-ml.so`）

**验证 GPU 设置**：
```bash
# 检查 NVIDIA 驱动程序
nvidia-smi

# 检查 NVML 库
ldconfig -p | grep libnvidia-ml

# 使用 gflow 测试 GPU 检测
gflowd up
ginfo
```

### 无 GPU 系统

gflow 在没有 GPU 的系统上也能完美工作：
- GPU 检测失败时优雅降级
- 除 GPU 分配外所有功能都可用
- 任务仍可在没有 `--gpus` 标志的情况下提交

## GPU 分配

### 请求 GPU

提交任务时请求 GPU：

```bash
# 请求 1 个 GPU
gbatch --gpus 1 python train.py

# 请求 2 个 GPU
gbatch --gpus 2 python multi_gpu_train.py

# 请求 4 个 GPU
gbatch --gpus 4 python distributed_train.py
```

### 自动 GPU 分配

当任务请求 GPU 时：
1. 调度器检查可用 GPU
2. 为任务分配特定的 GPU ID
3. 设置 `CUDA_VISIBLE_DEVICES` 环境变量
4. 任务仅看到其分配的 GPU（编号为 0、1、2、...）

**示例**：
```bash
# 提交请求 2 个 GPU 的任务
$ gbatch --gpus 2 nvidia-smi

# 检查分配
$ gqueue -f JOBID,NAME,NODES,NODELIST
JOBID    NAME                NODES    NODELIST(REASON)
42       brave-river-1234    2        1,2

# 在任务内，CUDA_VISIBLE_DEVICES=1,2
# 但 CUDA 会将它们重新编号为 0,1 供应用程序使用
```

### GPU 可见性

gflow 使用 `CUDA_VISIBLE_DEVICES` 控制 GPU 访问：

```python
# 在您的任务中（Python 示例）
import os
import torch

# gflow 自动设置这个
print(f"CUDA_VISIBLE_DEVICES: {os.environ.get('CUDA_VISIBLE_DEVICES')}")

# CUDA 仅看到分配的 GPU
print(f"Visible GPUs to CUDA: {torch.cuda.device_count()}")

# 正常使用 GPU（从 0 开始索引）
device = torch.device('cuda:0')  # 第一个分配的 GPU
```

**Bash 示例**：
```bash
#!/bin/bash
# GFLOW --gpus 2

echo "Allocated GPUs: $CUDA_VISIBLE_DEVICES"
nvidia-smi --query-gpu=index,name,memory.free --format=csv
python train.py
```

## GPU 调度

### 带 GPU 请求的任务队列

当没有 GPU 可用时任务等待：

```bash
# 系统有 2 个 GPU

# 任务 1：使用 2 个 GPU
$ gbatch --gpus 2 python long_train.py
Submitted batch job 1

# 任务 2：请求 1 个 GPU（必须等待）
$ gbatch --gpus 1 python train.py
Submitted batch job 2

$ gqueue
JOBID    NAME      ST    NODES    NODELIST(REASON)
1        job-1     R     2        0,1
2        job-2     PD    1        (Resources)
```

任务 2 等待直到任务 1 释放至少 1 个 GPU。

### 优先级和 GPU 分配

更高优先级的任务首先获得 GPU：

```bash
# 低优先级任务
gbatch --priority 5 --gpus 1 python task1.py

# 高优先级任务
gbatch --priority 100 --gpus 1 python urgent_task.py
```

当 GPU 可用时：
1. 调度器选择最高优先级的队列任务
2. 检查是否有足够的 GPU 空闲
3. 分配 GPU 并启动任务

### 部分 GPU 可用性

如果任务请求的 GPU 数量超过当前可用数量，它会等待：

```bash
# 系统有 4 个 GPU，3 个在使用中

# 这等待 4 个 GPU
gbatch --gpus 4 python distributed_train.py

$ gqueue
JOBID    NAME      ST    NODES    NODELIST(REASON)
5        job-5     PD    4        (Resources: Need 4 GPUs, only 1 available)
```

## 监控 GPU 使用

### 检查当前 GPU 分配

查看运行中任务的 GPU 分配：

```bash
$ gqueue -s Running -f JOBID,NAME,NODES,NODELIST
```

**示例输出**（当任务运行时）：
```
JOBID    NAME                NODES    NODELIST(REASON)
1        train-resnet        1        0
2        train-vit           1        1
3        train-bert          2        2,3
```

`NODES` 列显示每个任务请求的 GPU 数量，`NODELIST` 显示分配的特定 GPU ID。

### 系统范围的 GPU 状态

```bash
# 查看系统信息
$ ginfo

# 使用 nvidia-smi 进行实时监控
watch -n 1 nvidia-smi
```

### 每个任务的 GPU 使用

```bash
# 提交带 GPU 监控的任务
cat > monitor_gpu.sh << 'EOF'
#!/bin/bash
# GFLOW --gpus 1

echo "=== GPU Allocation ==="
echo "CUDA_VISIBLE_DEVICES: $CUDA_VISIBLE_DEVICES"

echo "=== GPU Details ==="
nvidia-smi --query-gpu=index,name,memory.total,memory.free,utilization.gpu \
           --format=csv

echo "=== Training ==="
python train.py
EOF

chmod +x monitor_gpu.sh
gbatch monitor_gpu.sh
```

检查日志：
```bash
cat ~/.local/share/gflow/logs/<job_id>.log
```

## 多 GPU 训练

### 数据并行训练（PyTorch）

```python
# train.py
import torch
import torch.nn as nn

# gflow 自动设置 CUDA_VISIBLE_DEVICES
device_count = torch.cuda.device_count()
print(f"Using {device_count} GPUs")

model = MyModel()
if device_count > 1:
    model = nn.DataParallel(model)
model = model.cuda()

# 正常训练
train(model)
```

使用多个 GPU 提交：
```bash
gbatch --gpus 2 python train.py
```

### 分布式训练（PyTorch）

```python
# distributed_train.py
import torch
import torch.distributed as dist

def main():
    # gflow 通过 CUDA_VISIBLE_DEVICES 分配 GPU
    world_size = torch.cuda.device_count()

    # 初始化进程组
    dist.init_process_group(backend='nccl', world_size=world_size)

    # 获取本地排名
    local_rank = dist.get_rank()
    torch.cuda.set_device(local_rank)

    # 训练代码
    train(local_rank)

if __name__ == '__main__':
    main()
```

提交：
```bash
gbatch --gpus 4 python distributed_train.py
```

### TensorFlow 多 GPU

```python
# tf_train.py
import tensorflow as tf

# 让 TensorFlow 看到所有分配的 GPU
gpus = tf.config.list_physical_devices('GPU')
print(f"Available GPUs: {len(gpus)}")

strategy = tf.distribute.MirroredStrategy()

with strategy.scope():
    model = create_model()
    model.compile(...)

model.fit(...)
```

提交：
```bash
gbatch --gpus 2 python tf_train.py
```

## 高级 GPU 管理

### GPU 内存考虑

即使 GPU"可用"，它们可能内存不足：

```bash
# 提交大型任务前检查 GPU 内存
nvidia-smi --query-gpu=memory.free --format=csv,noheader,nounits

# 示例：任务每个 GPU 需要 20GB
gbatch --gpus 1 python memory_intensive_train.py
```

**注意**：gflow 跟踪 GPU 分配，而不是内存使用。相应地规划。

### 独占 GPU 访问

每个任务获得其分配 GPU 的独占访问：
- 没有其他 gflow 任务可以使用它们
- gflow 外的其他进程仍可以访问它们
- 使用 `CUDA_VISIBLE_DEVICES` 确保隔离

### 混合 GPU/CPU 任务

同时运行 CPU 和 GPU 任务：

```bash
# 仅 CPU 任务
gbatch python cpu_task.py

# GPU 任务
gbatch --gpus 1 python gpu_task.py
```

CPU 任务不消耗 GPU 插槽，可与 GPU 任务并行运行。

## GPU 任务模式

### 顺序 GPU 管道

在阶段之间释放 GPU：

```bash
# 阶段 1：预处理（无 GPU）
ID1=$(gbatch --time 30 python preprocess.py | grep -oP '\d+')

# 阶段 2：训练（使用 GPU）
ID2=$(gbatch --depends-on $ID1 --gpus 1 --time 4:00:00 \
             python train.py | grep -oP '\d+')

# 阶段 3：评估（无 GPU）
gbatch --depends-on $ID2 --time 10 python evaluate.py
```

**优势**：GPU 在预处理和评估期间空闲。

### 并行多 GPU 实验

在不同 GPU 上并行运行实验：

```bash
# 每个获得一个 GPU
gbatch --gpus 1 --time 2:00:00 --config config1.yaml --name "exp1" python train.py
gbatch --gpus 1 --time 2:00:00 --config config2.yaml --name "exp2" python train.py
gbatch --gpus 1 --time 2:00:00 --config config3.yaml --name "exp3" python train.py
```

如果您有 4 个 GPU，前 4 个任务并行运行。

### 动态 GPU 扩展

从较少 GPU 开始，稍后扩展：

```bash
# 初始实验（1 个 GPU）
gbatch --gpus 1 --time 1:00:00 python train.py --test-run

# 完整训练（4 个 GPU）- 验证后提交
gbatch --gpus 4 --time 8:00:00 python train.py --full
```

### 带 GPU 的超参数扫描

```bash
# 在 4 个 GPU 上进行网格搜索
for lr in 0.001 0.01 0.1; do
    for batch_size in 32 64 128; do
        gbatch --gpus 1 --time 3:00:00 \
               --name "lr${lr}_bs${batch_size}" \
               python train.py --lr $lr --batch-size $batch_size
    done
done

# 监控 GPU 分配
watch -n 2 'gqueue -s Running,Queued -f JOBID,NAME,NODES,NODELIST'
```

## 故障排除

### 问题：任务没有获得 GPU

**可能原因**：

1. **忘记请求 GPU**：
   ```bash
   # 错误 - 没有请求 GPU
   gbatch python train.py

   # 正确
   gbatch --gpus 1 python train.py
   ```

2. **所有 GPU 在使用中**：
   ```bash
   # 检查分配
   gqueue -s Running -f NODES,NODELIST
   ginfo
   ```

3. **任务在队列中**：
   ```bash
   # 任务等待 GPU
   $ gqueue -j <job_id> -f JOBID,ST,NODES,NODELIST
   JOBID    ST    NODES    NODELIST(REASON)
   42       PD    1        (Resources)
   ```

### 问题：任务看到错误的 GPU

**检查 CUDA_VISIBLE_DEVICES**：
```bash
# 在您的任务脚本中
echo "CUDA_VISIBLE_DEVICES: $CUDA_VISIBLE_DEVICES"

# 应该与 gqueue 输出匹配
gqueue -f JOBID,NODELIST
```

### 问题：内存不足错误

**解决方案**：
1. 请求更多 GPU：`--gpus 2`
2. 在代码中减少批大小
3. 使用梯度累积
4. 启用混合精度训练（FP16）

**检查内存**：
```bash
nvidia-smi --query-gpu=memory.free,memory.used --format=csv
```

### 问题：GPU 利用率低

**可能原因**：
- 数据加载瓶颈（使用更多工作进程）
- CPU 预处理瓶颈
- 批大小太小
- 模型对 GPU 来说太小

**调试**：
```bash
# 监控 GPU 利用率
watch -n 1 nvidia-smi

# 检查任务日志中的瓶颈
tail -f ~/.local/share/gflow/logs/<job_id>.log
```

## 最佳实践

1. **仅请求需要的 GPU**：不要过度分配资源
2. **监控 GPU 使用**：使用 `nvidia-smi` 验证利用率
3. **优化数据加载**：防止 GPU 饥饿
4. **使用混合精度**：使用 FP16 减少内存使用
5. **高效批处理任务**：按 GPU 需求分组相似任务
6. **提前释放 GPU**：使用依赖链接 CPU/GPU 阶段
7. **先在 1 个 GPU 上测试**：扩展到多个 GPU 前验证
8. **设置时间限制**：通过失控任务防止 GPU 被占用
9. **记录 GPU 统计**：在任务日志中包含 GPU 信息
10. **清理检查点**：使用 GPU 时管理磁盘空间

## 性能提示

### 最大化 GPU 利用率

```python
# 增加批大小
train_loader = DataLoader(dataset, batch_size=128, num_workers=8)

# 使用 pin_memory 加快传输
train_loader = DataLoader(dataset, batch_size=128, pin_memory=True)

# 启用 AMP 进行混合精度
from torch.cuda.amp import autocast, GradScaler
scaler = GradScaler()

with autocast():
    output = model(input)
    loss = criterion(output, target)
```

### 高效的多 GPU 使用

```python
# 使用 DistributedDataParallel 而不是 DataParallel
from torch.nn.parallel import DistributedDataParallel as DDP

# 更高效的通信
model = DDP(model, device_ids=[local_rank])
```

### 监控和优化

```bash
#!/bin/bash
# GFLOW --gpus 1

# 训练前记录 GPU 统计
nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv -l 10 > gpu_stats.log &
GPU_MONITOR_PID=$!

# 运行训练
python train.py

# 停止监控
kill $GPU_MONITOR_PID
```

## 参考

### 环境变量

| 变量 | 设置者 | 描述 |
|------|--------|------|
| `CUDA_VISIBLE_DEVICES` | gflow | 逗号分隔的 GPU ID（例如"0,1"） |

### GPU 相关命令

```bash
# 检查系统 GPU
ginfo

# 使用 GPU 提交任务
gbatch --gpus <N> ...

# 检查 GPU 分配
gqueue -f JOBID,NODES,NODELIST

# 监控运行中的 GPU 任务
gqueue -s Running -f JOBID,NODES,NODELIST

# 监控系统 GPU
nvidia-smi
watch -n 1 nvidia-smi
```

## 另见

- [任务提交](./job-submission) - 完整的任务提交指南
- [任务依赖](./job-dependencies) - 工作流管理
- [时间限制](./time-limits) - 任务超时管理
- [快速参考](../reference/quick-reference) - 命令速查表
