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

# Scheduling
gbatch --priority 50 python urgent.py
gbatch --name my-run python train.py

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

## Time Format (`--time`)

- `HH:MM:SS` (e.g. `2:30:00`)
- `MM:SS` (e.g. `5:30`)
- `MM` minutes (e.g. `30`)

Note: a single number is **minutes**. Use `0:30` for 30 seconds.

## Memory Format (`--memory`)

- `100` (MB)
- `1024M`
- `2G`

## Script Directives

When submitting a script, `gbatch` can parse a small subset of options from lines like:

```bash
#!/bin/bash
# GFLOW --gpus=1
# GFLOW --time=2:00:00
# GFLOW --memory=4G
# GFLOW --priority=20
# GFLOW --conda-env=myenv
# GFLOW --depends-on=123
```

Notes:

- CLI flags override script directives.
- Script directives support only `--depends-on` (single dependency).
