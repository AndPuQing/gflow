# GPU Management

gflow detects NVIDIA GPUs (via NVML) and allocates them to jobs by setting `CUDA_VISIBLE_DEVICES`.

## Quick Start

```bash
# Start the daemon (if not already running)
gflowd up

# See availability + current allocations
ginfo

# Submit a GPU job
gbatch --gpus 1 python train.py

# Track jobs and allocations
gqueue -s Running,Queued -f JOBID,NAME,ST,NODES,NODELIST(REASON)
```

## Inspect GPUs

```bash
ginfo
```

Example output:
```
PARTITION  GPUS  NODES  STATE      JOB(REASON)
gpu        1     1      idle
gpu        1     0      allocated  5 (train-resnet)
```

- `NODES` shows the physical GPU indices.
- If a GPU is busy but not allocated by gflow, it may appear with a reason (when available).

Non-gflow GPU usage:
- If NVML reports running compute processes on a GPU, gflow treats it as unavailable (often shown as `Unmanaged`) and will not allocate it.
- gflow does not preempt/kill non-gflow processes; jobs wait until the GPU becomes idle.

If you need per-GPU restriction status (allowed vs restricted):

```bash
gctl show-gpus
```

### Requirements

- NVIDIA GPU(s) + driver
- NVML library available (`libnvidia-ml.so`)

Quick check:

```bash
nvidia-smi
gflowd up
ginfo
```

On systems without GPUs, gflow still works; only GPU allocation is unavailable.

## Request GPUs

```bash
gbatch --gpus 1 python train.py
gbatch --gpus 2 python multi_gpu_train.py
```

When a job starts, gflow assigns **physical GPU indices** and exports them via `CUDA_VISIBLE_DEVICES` (which frameworks typically renumber starting from `0`).

To see allocated GPU IDs:

```bash
gqueue -s Running -f JOBID,NAME,ST,NODES,NODELIST(REASON)
gjob show <job_id>
```

### GPU Visibility

```bash
#!/bin/bash
# GFLOW --gpus 2

echo "CUDA_VISIBLE_DEVICES=$CUDA_VISIBLE_DEVICES"
python train.py
```

## Restrict Which GPUs gflow Uses

Limit which physical GPUs the scheduler is allowed to allocate (affects new allocations only):

```bash
gctl set-gpus 0,2
gctl show-gpus

# Or via daemon CLI flag (overrides config)
gflowd restart --gpus 0-3
```

See also: [Configuration -> GPU Selection](./configuration#gpu-selection).

## Troubleshooting

### Job not getting GPU

```bash
ginfo
gqueue -j <job_id> -f JOBID,ST,NODES,NODELIST(REASON)
gctl show-gpus
```

### Job sees wrong GPUs

```bash
echo "CUDA_VISIBLE_DEVICES=$CUDA_VISIBLE_DEVICES"
gqueue -f JOBID,NODELIST(REASON)
```

### Out of memory

```bash
nvidia-smi --query-gpu=memory.free,memory.used --format=csv
```

## See Also

- [Job Submission](./job-submission) - Complete job submission guide
- [Job Dependencies](./job-dependencies) - Workflow management
- [Time Limits](./time-limits) - Job timeout management
- [Quick Reference](../reference/quick-reference) - Command cheat sheet
