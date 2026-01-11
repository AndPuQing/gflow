# GPU Management

This guide covers how gflow manages GPU resources, from detection to allocation and monitoring.

## Overview

gflow provides automatic GPU detection, allocation, and management for NVIDIA GPUs through the NVML library. It ensures efficient GPU utilization across multiple jobs while preventing resource conflicts.

## GPU Detection

### Checking Available GPUs

View system GPU information:

```bash
$ ginfo
```

Example output:
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

**Information displayed**:
- Total number of GPUs in the system
- Number of currently available (unused) GPUs
- GPU model and UUID for each device
- Current allocation status (available or in use by which job)
- **Enhanced display**: Shows GPU allocations organized by job, making it easy to see which jobs are using which GPUs

### Requirements

**System requirements**:
- NVIDIA GPU(s)
- NVIDIA drivers installed
- NVML library available (`libnvidia-ml.so`)

**Verify GPU setup**:
```bash
# Check NVIDIA driver
nvidia-smi

# Check NVML library
ldconfig -p | grep libnvidia-ml

# Test GPU detection with gflow
gflowd up
ginfo
```

### No GPU Systems

gflow works perfectly fine on systems without GPUs:
- GPU detection fails gracefully
- All features work except GPU allocation
- Jobs can still be submitted without `--gpus` flag

## GPU Allocation

### Requesting GPUs

Request GPUs when submitting jobs:

```bash
# Request 1 GPU
gbatch --gpus 1 python train.py

# Request 2 GPUs
gbatch --gpus 2 python multi_gpu_train.py

# Request 4 GPUs
gbatch --gpus 4 python distributed_train.py
```

### Automatic GPU Assignment

When a job requests GPUs:
1. Scheduler checks for available GPUs
2. Assigns specific GPU IDs to the job
3. Sets `CUDA_VISIBLE_DEVICES` environment variable
4. Job sees only its allocated GPUs (numbered 0, 1, 2, ...)

**Example**:
```bash
# Submit job requesting 2 GPUs
$ gbatch --gpus 2 nvidia-smi

# Check allocation
$ gqueue -f JOBID,NAME,NODES,NODELIST
JOBID    NAME                NODES    NODELIST(REASON)
42       brave-river-1234    2        1,2

# Inside the job, CUDA_VISIBLE_DEVICES=1,2
# But CUDA will renumber them as 0,1 for the application
```

### GPU Visibility

gflow uses `CUDA_VISIBLE_DEVICES` to control GPU access:

```python
# In your job (Python example)
import os
import torch

# gflow sets this automatically
print(f"CUDA_VISIBLE_DEVICES: {os.environ.get('CUDA_VISIBLE_DEVICES')}")

# CUDA sees only allocated GPUs
print(f"Visible GPUs to CUDA: {torch.cuda.device_count()}")

# Use GPUs normally (indexed from 0)
device = torch.device('cuda:0')  # First allocated GPU
```

**Bash example**:
```bash
#!/bin/bash
# GFLOW --gpus 2

echo "Allocated GPUs: $CUDA_VISIBLE_DEVICES"
nvidia-smi --query-gpu=index,name,memory.free --format=csv
python train.py
```

## GPU Scheduling

### Job Queue with GPU Requests

Jobs wait for GPUs when none are available:

```bash
# System has 2 GPUs

# Job 1: Uses 2 GPUs
$ gbatch --gpus 2 python long_train.py
Submitted batch job 1

# Job 2: Requests 1 GPU (must wait)
$ gbatch --gpus 1 python train.py
Submitted batch job 2

$ gqueue
JOBID    NAME      ST    NODES    NODELIST(REASON)
1        job-1     R     2        0,1
2        job-2     PD    1        (Resources)
```

Job 2 waits until Job 1 releases at least 1 GPU.

### Priority and GPU Allocation

Higher priority jobs get GPUs first:

```bash
# Low priority job
gbatch --priority 5 --gpus 1 python task1.py

# High priority job
gbatch --priority 100 --gpus 1 python urgent_task.py
```

When GPUs become available:
1. Scheduler selects highest priority queued job
2. Checks if enough GPUs are free
3. Allocates GPUs and starts the job

### Partial GPU Availability

If a job requests more GPUs than currently available, it waits:

```bash
# System has 4 GPUs, 3 in use

# This waits for 4 GPUs
gbatch --gpus 4 python distributed_train.py

$ gqueue
JOBID    NAME      ST    NODES    NODELIST(REASON)
5        job-5     PD    4        (Resources: Need 4 GPUs, only 1 available)
```

## Monitoring GPU Usage

### Check Current GPU Allocation

View GPU allocation for running jobs:

```bash
$ gqueue -s Running -f JOBID,NAME,NODES,NODELIST
```

**Example output** (when jobs are running):
```
JOBID    NAME                NODES    NODELIST(REASON)
1        train-resnet        1        0
2        train-vit           1        1
3        train-bert          2        2,3
```

The `NODES` column shows how many GPUs each job requested, and `NODELIST` shows the specific GPU IDs allocated.

### System-wide GPU Status

```bash
# View system info
$ ginfo

# Use nvidia-smi for real-time monitoring
watch -n 1 nvidia-smi
```

### Per-job GPU Usage

```bash
# Submit job with GPU monitoring
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

Check the log:
```bash
cat ~/.local/share/gflow/logs/<job_id>.log
```

## Multi-GPU Training

### Data Parallel Training (PyTorch)

```python
# train.py
import torch
import torch.nn as nn

# gflow sets CUDA_VISIBLE_DEVICES automatically
device_count = torch.cuda.device_count()
print(f"Using {device_count} GPUs")

model = MyModel()
if device_count > 1:
    model = nn.DataParallel(model)
model = model.cuda()

# Train normally
train(model)
```

Submit with multiple GPUs:
```bash
gbatch --gpus 2 python train.py
```

### Distributed Training (PyTorch)

```python
# distributed_train.py
import torch
import torch.distributed as dist

def main():
    # gflow allocates GPUs via CUDA_VISIBLE_DEVICES
    world_size = torch.cuda.device_count()

    # Initialize process group
    dist.init_process_group(backend='nccl', world_size=world_size)

    # Get local rank
    local_rank = dist.get_rank()
    torch.cuda.set_device(local_rank)

    # Training code
    train(local_rank)

if __name__ == '__main__':
    main()
```

Submit:
```bash
gbatch --gpus 4 python distributed_train.py
```

### TensorFlow Multi-GPU

```python
# tf_train.py
import tensorflow as tf

# Let TensorFlow see all allocated GPUs
gpus = tf.config.list_physical_devices('GPU')
print(f"Available GPUs: {len(gpus)}")

strategy = tf.distribute.MirroredStrategy()

with strategy.scope():
    model = create_model()
    model.compile(...)

model.fit(...)
```

Submit:
```bash
gbatch --gpus 2 python tf_train.py
```

## Advanced GPU Management

### GPU Memory Considerations

Even if GPUs are "available", they might have insufficient memory:

```bash
# Check GPU memory before submitting large jobs
nvidia-smi --query-gpu=memory.free --format=csv,noheader,nounits

# Example: Job needs 20GB per GPU
gbatch --gpus 1 python memory_intensive_train.py
```

**Note**: gflow tracks GPU allocation, not memory usage. Plan accordingly.

### Exclusive GPU Access

Each job gets exclusive access to its allocated GPUs:
- No other gflow job can use them
- Other processes (outside gflow) can still access them
- Use `CUDA_VISIBLE_DEVICES` to ensure isolation

### Mixed GPU/CPU Jobs

Run CPU and GPU jobs simultaneously:

```bash
# CPU-only job
gbatch python cpu_task.py

# GPU job
gbatch --gpus 1 python gpu_task.py
```

CPU jobs don't consume GPU slots and can run in parallel with GPU jobs.

## GPU Job Patterns

### Sequential GPU Pipeline

Release GPUs between stages:

```bash
# Stage 1: Preprocessing (no GPU)
ID1=$(gbatch --time 30 python preprocess.py | grep -oP '\d+')

# Stage 2: Training (uses GPU)
ID2=$(gbatch --depends-on $ID1 --gpus 1 --time 4:00:00 \
             python train.py | grep -oP '\d+')

# Stage 3: Evaluation (no GPU)
gbatch --depends-on $ID2 --time 10 python evaluate.py
```

**Benefit**: GPU is free during preprocessing and evaluation.

### Parallel Multi-GPU Experiments

Run experiments in parallel on different GPUs:

```bash
# Each gets one GPU
gbatch --gpus 1 --time 2:00:00 --config config1.yaml --name "exp1" python train.py
gbatch --gpus 1 --time 2:00:00 --config config2.yaml --name "exp2" python train.py
gbatch --gpus 1 --time 2:00:00 --config config3.yaml --name "exp3" python train.py
```

If you have 4 GPUs, the first 4 jobs run in parallel.

### Dynamic GPU Scaling

Start with fewer GPUs, scale up later:

```bash
# Initial experiment (1 GPU)
gbatch --gpus 1 --time 1:00:00 python train.py --test-run

# Full training (4 GPUs) - submit after validation
gbatch --gpus 4 --time 8:00:00 python train.py --full
```

### Hyperparameter Sweep with GPUs

```bash
# Grid search across 4 GPUs
for lr in 0.001 0.01 0.1; do
    for batch_size in 32 64 128; do
        gbatch --gpus 1 --time 3:00:00 \
               --name "lr${lr}_bs${batch_size}" \
               python train.py --lr $lr --batch-size $batch_size
    done
done

# Monitor GPU allocation
watch -n 2 'gqueue -s Running,Queued -f JOBID,NAME,NODES,NODELIST'
```

## Troubleshooting

### Issue: Job not getting GPU

**Possible causes**:

1. **Forgot to request GPU**:
   ```bash
   # Wrong - no GPU requested
   gbatch python train.py

   # Correct
   gbatch --gpus 1 python train.py
   ```

2. **All GPUs in use**:
   ```bash
   # Check allocation
   gqueue -s Running -f NODES,NODELIST
   ginfo
   ```

3. **Job is queued**:
   ```bash
   # Job waits for GPU
   $ gqueue -j <job_id> -f JOBID,ST,NODES,NODELIST
   JOBID    ST    NODES    NODELIST(REASON)
   42       PD    1        (Resources)
   ```

### Issue: Job sees wrong GPUs

**Check CUDA_VISIBLE_DEVICES**:
```bash
# In your job script
echo "CUDA_VISIBLE_DEVICES: $CUDA_VISIBLE_DEVICES"

# Should match gqueue output
gqueue -f JOBID,NODELIST
```

### Issue: Out of memory error

**Solutions**:
1. Request more GPUs: `--gpus 2`
2. Reduce batch size in your code
3. Use gradient accumulation
4. Enable mixed precision training (FP16)

**Check memory**:
```bash
nvidia-smi --query-gpu=memory.free,memory.used --format=csv
```

### Issue: GPU utilization low

**Possible causes**:
- Data loading bottleneck (use more workers)
- CPU preprocessing bottleneck
- Small batch size
- Model too small for GPU

**Debug**:
```bash
# Monitor GPU utilization
watch -n 1 nvidia-smi

# Check job logs for bottlenecks
tail -f ~/.local/share/gflow/logs/<job_id>.log
```

## Best Practices

1. **Request only needed GPUs**: Don't over-allocate resources
2. **Monitor GPU usage**: Use `nvidia-smi` to verify utilization
3. **Optimize data loading**: Prevent GPU starvation
4. **Use mixed precision**: Reduce memory usage with FP16
5. **Batch jobs efficiently**: Group similar GPU requirements
6. **Release GPUs early**: Use dependencies to chain CPU/GPU stages
7. **Test on 1 GPU first**: Validate before scaling to multiple GPUs
8. **Set time limits**: Prevent GPU hogging by runaway jobs
9. **Log GPU stats**: Include GPU info in job logs
10. **Clean up checkpoints**: Manage disk space when using GPUs

## Performance Tips

### Maximize GPU Utilization

```python
# Increase batch size
train_loader = DataLoader(dataset, batch_size=128, num_workers=8)

# Use pin_memory for faster transfers
train_loader = DataLoader(dataset, batch_size=128, pin_memory=True)

# Enable AMP for mixed precision
from torch.cuda.amp import autocast, GradScaler
scaler = GradScaler()

with autocast():
    output = model(input)
    loss = criterion(output, target)
```

### Efficient Multi-GPU Usage

```python
# Use DistributedDataParallel instead of DataParallel
from torch.nn.parallel import DistributedDataParallel as DDP

# More efficient communication
model = DDP(model, device_ids=[local_rank])
```

### Monitor and Optimize

```bash
#!/bin/bash
# GFLOW --gpus 1

# Log GPU stats before training
nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv -l 10 > gpu_stats.log &
GPU_MONITOR_PID=$!

# Run training
python train.py

# Stop monitoring
kill $GPU_MONITOR_PID
```

## Reference

### Environment Variables

| Variable | Set By | Description |
|----------|--------|-------------|
| `CUDA_VISIBLE_DEVICES` | gflow | Comma-separated GPU IDs (e.g., "0,1") |

### GPU-Related Commands

```bash
# Check system GPUs
ginfo

# Submit job with GPUs
gbatch --gpus <N> ...

# Check GPU allocation
gqueue -f JOBID,NODES,NODELIST

# Monitor running GPU jobs
gqueue -s Running -f JOBID,NODES,NODELIST

# Monitor system GPUs
nvidia-smi
watch -n 1 nvidia-smi
```

## See Also

- [Job Submission](./job-submission) - Complete job submission guide
- [Job Dependencies](./job-dependencies) - Workflow management
- [Time Limits](./time-limits) - Job timeout management
- [Quick Reference](../reference/quick-reference) - Command cheat sheet
