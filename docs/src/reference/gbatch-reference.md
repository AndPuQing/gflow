# gbatch Reference

`gbatch` submits jobs to the scheduler (similar to Slurm `sbatch`).

## Usage

```bash
gbatch [options] <script>
gbatch [options] <command> [args...]
gbatch new <name>
gbatch completion <shell>
```

## Common Options

```bash
# Resources
gbatch --gpus 1 python train.py
gbatch --time 2:00:00 python train.py
gbatch --memory 8G python train.py
gbatch --gpu-memory 20G --shared --gpus 1 python train.py

# Scheduling
gbatch --priority 50 python urgent.py
gbatch --name my-run python train.py
gbatch --project ml-research python train.py
gbatch --notify-email alice@example.com --notify-on job_failed,job_timeout python train.py

# Environment
gbatch --conda-env myenv python script.py

# Dependencies
gbatch --depends-on <job_id|@|@~N> python next.py
gbatch --depends-on-all 1,2,3 python merge.py     # AND
gbatch --depends-on-any 4,5 python fallback.py    # OR
gbatch --depends-on 123 --no-auto-cancel python next.py

Shorthands: `@` = most recent job, `@~N` = Nth most recent submission.

# Arrays
gbatch --array 1-10 python task.py --i '$GFLOW_ARRAY_TASK_ID'

# Params (cartesian product)
gbatch --param lr=0.001,0.01 --param bs=32,64 python train.py --lr {lr} --batch-size {bs}
gbatch --param-file params.csv --name-template 'run_{id}' python train.py --id {id}
gbatch --max-concurrent 2 --param lr=0.001,0.01 python train.py --lr {lr}

# Preview
gbatch --dry-run --gpus 1 python train.py
```

## Slurm-Compatible Aliases

To ease migration from Slurm `sbatch`, `gbatch` accepts a few common flag aliases:

- `--nice` → `--priority`
- `--job-name` (or `-J`) → `--name`
- `--gres` → `--gpus` (expects an integer GPU count, e.g. `--gres 2`)
- `--dependency` → `--depends-on`
- `--time-limit` / `--timelimit` → `--time`

## Time Format (`--time`)

- `HH:MM:SS` (e.g. `2:30:00`)
- `MM:SS` (e.g. `5:30`)
- `MM` minutes (e.g. `30`)

Note: a single number is **minutes**. Use `0:30` for 30 seconds.

## Memory Format (`--memory`)

- `100` (MB)
- `1024M`
- `2G`

Aliases: `--max-mem`, `--max-memory`.

`--memory` controls host RAM, not GPU VRAM.

## GPU Memory Format (`--gpu-memory`)

- `8192` (MB)
- `16384M`
- `24G`

Aliases: `--max-gpu-mem`, `--max-gpu-memory`.

`--gpu-memory` controls per-GPU VRAM.

## Shared GPU Mode (`--shared`)

- Use `--shared` to allow jobs to share the same GPU with other shared jobs.
- Shared jobs must specify `--gpu-memory`.
- `--shared` never mixes with exclusive jobs on the same GPU.

## Script Directives

When submitting a script, `gbatch` can parse a small subset of options from lines like:

```bash
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --shared
# GFLOW --time=2:00:00
# GFLOW --memory=4G
# GFLOW --gpu-memory=20G
# GFLOW --priority=20
# GFLOW --conda-env=myenv
# GFLOW --depends-on=123
# GFLOW --project=ml-research
# GFLOW --notify-email=alice@example.com
# GFLOW --notify-on=job_failed,job_timeout
```

Notes:

- CLI flags override script directives.
- Script directives support only `--depends-on` (single dependency).

## Project Tracking (`--project`)

- Use `-P/--project <code>` to attach an optional project code to submitted jobs.
- Project values are normalized by trimming surrounding whitespace; blank values are treated as unset.
- Maximum length is 64 characters.
- Project value is immutable after submission.
- CLI `--project` overrides `# GFLOW --project=...` in scripts.

## Per-Job Notifications (`--notify-email`, `--notify-on`)

- Use `--notify-email <address>` multiple times to attach job-specific email recipients.
- Use `--notify-on <event1,event2,...>` to choose which events trigger those emails.
- If `--notify-email` is set but `--notify-on` is omitted, gflow defaults to `job_completed`, `job_failed`, `job_timeout`, and `job_cancelled`.
- CLI flags are merged with script directives for recipients; if CLI `--notify-on` is provided it overrides script events.
- Delivery still uses the global SMTP transports configured under [Notifications](../user-guide/notifications).
