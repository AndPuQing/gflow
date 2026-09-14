# ginfo Reference

`ginfo` shows scheduler and GPU allocation information.

If a GPU is occupied by a non-gflow compute process, it is shown as allocated with an `unmanaged(pid=…)` reason, and gflow will not allocate it until it becomes idle. `ginfo` also prints a table of those processes with their memory, utilization, age, and the exact command that releases the GPU.

## Usage

```bash
# Display as table
ginfo
```

## Examples

```bash
ginfo
watch -n 2 ginfo
```

## Output

The top table mirrors `sinfo`: partition, GPU count, node (GPU) indices, state, and the job or reason using each card.

```
PARTITION  GPUS  NODES  STATE      JOB(REASON)
gpu        1     0      allocated  (unmanaged(pid=3471817))
gpu        1     1      idle
```

Below it, any GPU blocked by a non-gflow process is listed with the detail needed to reclaim it:

```
Non-gflow GPU processes (gflow will not allocate a GPU while these are attached):
GPU  PID      GPU MEM  UTIL  AGE  NOTE           RELEASE
gpu  3471817  642M     0%    48h  idle leftover  gctl gpu-process ignore --gpu 0 --pid 3471817
Release a card without touching the process: run the RELEASE command for that PID.
```

- `GPU MEM` / `UTIL` come from NVML; `?` means the driver did not report a value.
- `AGE` is the age of the host process (Linux only).
- `NOTE` marks `idle leftover` when the process has held the GPU for over an hour while running no kernels — usually a leaked process rather than real work.
- `RELEASE` is the command that lets the scheduler use that GPU again. See [gctl Reference](./gctl-reference).

Runtime-only ignore overrides currently in effect are listed at the end, with their `unignore` command:

```
Active GPU process ignore overrides (runtime-only; cleared when gflowd restarts):
  gpu=0 pid=3471817  (undo: gctl gpu-process unignore --gpu 0 --pid 3471817)
```

## Options

- `-v/-vv/-q`: adjust verbosity
- `--config <path>`: use a custom config file (hidden)
