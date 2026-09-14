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
- 如果 NVML 检测到某张 GPU 上有运行中的计算进程，gflow 会将其视为不可用（显示为 `unmanaged(pid=…)`），不会去分配这张卡。
- gflow 不会抢占/终止非 gflow 进程；任务只会等待 GPU 变为空闲后再运行。

`ginfo` 会列出每个阻塞进程的内存、利用率、存活时长，以及释放该 GPU 的命令。若某个进程占用 GPU 超过一小时却几乎没有执行 kernel，会被标记为 `idle leftover`。详见[释放被非 gflow 进程占用的 GPU](#释放被非-gflow-进程占用的-gpu)。

如需查看每张 GPU 是否被限制（allowed vs restricted）：

```bash
gctl show-gpus
```

### 释放被非 gflow 进程占用的 GPU

有时某个进程占着 GPU 却什么都没做——比如崩溃的训练任务，或一个只建了 CUDA context 的 Python 会话。此时可以告诉 gflow 忽略该进程，让这张卡重新可被调度，而不需要动那个进程：

```bash
# ginfo 会为每个阻塞的 PID 打印可直接执行的命令
ginfo

# 例如一个占用 642 MiB、利用率 0%、已挂 2 天的进程
gctl gpu-process ignore --gpu 0 --pid 3471817
```

如果进程真的在跑（例如占用 80 GiB、利用率 99%），就应该保留它：忽略它会让 gflow 把作业调度到一张实际正在使用的卡上。

关于这个 override：

- 它**仅在运行时生效**：`gflowd` 重启或 reload 后会清空，不会跨重启保留。
- 用 `gctl gpu-process list` 查看当前生效的规则；`ginfo` 的输出末尾也会列出。
- 用 `gctl gpu-process unignore --gpu <index> --pid <pid>` 撤销。进程已退出时，对应规则会被自动丢弃。

`gbatch` 在提交作业后也会打印一段被阻塞 GPU 的摘要（含释放命令），避免可用卡数被长期压缩而无人察觉。

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

### GPU 共享模式

当你希望多个任务共用同一张物理 GPU 时，可使用共享模式。

```bash
gbatch --gpus 1 --shared --gpu-memory 20G python train.py
```

- `--shared` 任务只会和其他 `--shared` 任务共享。
- `--shared` 必须配合每卡显存上限 `--gpu-memory`（别名：`--max-gpu-mem`）。
- `--memory`（`--max-mem`）仍然表示主机内存，不是 GPU 显存。

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

另见：[配置 -> GPU 选择](./configuration#gpu-选择)。

## 选择 GPU 分配策略

当任务可用 GPU 不止一张时，可以选择 gflow 的分配方式：

- `sequential`（默认）：优先分配低索引 GPU。
- `random`：随机化可用 GPU 的选择顺序。

```toml
[daemon]
gpu_allocation_strategy = "sequential"
# gpu_allocation_strategy = "random"
```

也可在启动守护进程时覆盖：

```bash
gflowd up --gpu-allocation-strategy random
```

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

若共享任务出现显存 OOM，请优先检查 `--gpu-memory` 是否已设置且数值是否合理。

## 另见

- [任务提交](./job-submission) - 完整的任务提交指南
- [任务依赖](./job-dependencies) - 工作流管理
- [时间限制](./time-limits) - 任务超时管理
- [快速参考](../reference/quick-reference) - 命令速查表
