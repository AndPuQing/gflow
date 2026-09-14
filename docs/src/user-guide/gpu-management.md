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
- If NVML reports running compute processes on a GPU, gflow treats it as unavailable (shown as `unmanaged(pid=…)`) and will not allocate it.
- gflow does not preempt/kill non-gflow processes; jobs wait until the GPU becomes idle.

`ginfo` lists each blocking process with its memory, utilization, and age, plus the command that releases the GPU. A process that has held a card for over an hour while running no kernels is flagged `idle leftover`. See [Reclaim a GPU blocked by a non-gflow process](#reclaim-a-gpu-blocked-by-a-non-gflow-process).

If you need per-GPU restriction status (allowed vs restricted):

```bash
gctl show-gpus
```

### Reclaim a GPU blocked by a non-gflow process

Sometimes a process holds a GPU but is doing nothing useful — a crashed training
job, or a stray Python session with a CUDA context. In that case you can tell
gflow to ignore that process so the card becomes schedulable again, without
touching the process itself:

```bash
# ginfo prints the exact command for each blocking PID
ginfo

# e.g. for a 642 MiB / 0%-utilization process that has been attached for 2 days
gctl gpu-process ignore --gpu 0 --pid 3471817
```

A GPU held by a genuinely busy process (for example 80 GiB and 99% utilization)
should be left alone: ignoring it would let gflow schedule onto a card that is
already in use.

Things to know about this override:

- It is **runtime-only**: `gflowd` restart or reload clears it, and it is not
  persisted across restarts.
- List the overrides currently in effect with `gctl gpu-process list`; `ginfo`
  shows them at the end of its output too.
- Undo one with `gctl gpu-process unignore --gpu <index> --pid <pid>`. An
  override whose process has exited is dropped automatically.

`gbatch` also prints a short summary of blocked GPUs (with the release command)
after you submit a job, so a permanently reduced GPU count does not go unnoticed.

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

### Shared GPU Mode

Use shared mode when you want multiple jobs to co-locate on one physical GPU.

```bash
gbatch --gpus 1 --shared --gpu-memory 20G python train.py
```

- `--shared` jobs only share with other `--shared` jobs.
- `--shared` requires a per-GPU VRAM limit via `--gpu-memory` (alias: `--max-gpu-mem`).
- `--memory` (`--max-mem`) is still host RAM, not GPU VRAM.

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

## Choose GPU Allocation Strategy

When multiple GPUs are available for a job, you can choose how gflow selects them:

- `sequential` (default): picks lower indices first.
- `random`: randomizes GPU selection order.

```toml
[daemon]
gpu_allocation_strategy = "sequential"
# gpu_allocation_strategy = "random"
```

Or override on daemon startup:

```bash
gflowd up --gpu-allocation-strategy random
```

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

If shared jobs fail with OOM, verify `--gpu-memory` is set and sized appropriately for each job.

## See Also

- [Job Submission](./job-submission) - Complete job submission guide
- [Job Dependencies](./job-dependencies) - Workflow management
- [Time Limits](./time-limits) - Job timeout management
- [Quick Reference](../reference/quick-reference) - Command cheat sheet
