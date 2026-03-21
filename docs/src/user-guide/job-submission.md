# Job Submission

Submit jobs with `gbatch` (similar to Slurm `sbatch`). You can submit a command directly or run a script.

::: tip
Use direct commands for short, single-step work. Switch to a script when the command needs setup, environment activation, or multiple shell steps.
:::

## Quick Start

```bash
gbatch python train.py
gbatch --gpus 1 --time 2:00:00 --name train-resnet python train.py
gbatch --project ml-research python train.py
```

## Submit a Command

```bash
gbatch python train.py --epochs 100 --lr 0.01
```

For complex shell logic, prefer a script file.

## Submit a Script

```bash
cat > train.sh << 'EOF'
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --time=2:00:00

python train.py
EOF

chmod +x train.sh
gbatch train.sh
```

::: details Supported script directives

Only a small subset of options are parsed from scripts:

- `# GFLOW --gpus=<N>`
- `# GFLOW --shared`
- `# GFLOW --time=<TIME>`
- `# GFLOW --memory=<LIMIT>`
- `# GFLOW --gpu-memory=<LIMIT>`
- `# GFLOW --priority=<N>`
- `# GFLOW --conda-env=<ENV>`
- `# GFLOW --depends-on=<job_id|@|@~N>` (single dependency only)
- `# GFLOW --project=<CODE>`
:::

::: info
CLI flags override script directives.
:::

### Memory Semantics

- `--memory` (`--max-mem` / `--max-memory`) limits host RAM.
- `--gpu-memory` (`--max-gpu-mem` / `--max-gpu-memory`) limits per-GPU VRAM.
- Shared jobs must set both `--shared` and `--gpu-memory`.

::: warning
Shared GPU mode is incomplete unless both `--shared` and `--gpu-memory` are set.
:::

## Common Options

```bash
# GPUs
gbatch --gpus 1 python train.py

# Time limit
gbatch --time 30 python quick.py

# Shared GPU mode (must set --gpu-memory)
gbatch --gpus 1 --shared --gpu-memory 20G python train.py

# Priority
gbatch --priority 50 python urgent.py

# Conda env
gbatch --conda-env myenv python script.py

# Project code
gbatch --project ml-research python train.py

# Dependencies
gbatch --depends-on <job_id|@|@~N> python next.py
gbatch --depends-on-all 1,2,3 python merge.py
gbatch --depends-on-any 4,5 python process_first_success.py

# Shorthands:
# - @    = most recently submitted job
# - @~N  = Nth most recent submission (e.g. @~1 is previous)

# Disable auto-cancel on dependency failure
gbatch --depends-on <job_id> --no-auto-cancel python next.py

# Preview without submitting
gbatch --dry-run --gpus 1 python train.py
```

::: details Dependency shorthands
- `@` refers to the most recently submitted job.
- `@~N` refers to the Nth most recent submission. For example, `@~1` means the previous submission.
:::

::: info
Project values are immutable after submission.
:::

## Job Arrays

```bash
gbatch --array 1-10 python process.py --task '$GFLOW_ARRAY_TASK_ID'
```

## Monitor and Logs

```bash
# Jobs and allocations
gqueue -f JOBID,NAME,ST,NODES,NODELIST(REASON)

# Details for one job (includes GPUIDs)
gjob show <job_id>

# Logs
tail -f ~/.local/share/gflow/logs/<job_id>.log
```

## Adjust or Resubmit

- Update queued/held jobs: `gjob update <job_id> ...`
- Resubmit a job: `gjob redo <job_id>` (use `--cascade` to redo dependents)

## See Also

- [Job Dependencies](./job-dependencies) - Workflows and dependency modes
- [Time Limits](./time-limits) - Time format and behavior
- [GPU Management](./gpu-management) - Allocation details
