# Tips

Small tricks that make gflow workflows faster and safer.

## Dependency Shortcuts (`@`)

Use `@` to refer to recent submissions:

- `@`: most recently submitted job
- `@~1`: previous submission
- `@~2`: two submissions ago

```bash
gbatch --time 10 python preprocess.py
gbatch --depends-on @ --gpus 1 --time 4:00:00 python train.py
gbatch --depends-on @ --time 10 python evaluate.py
```

`@` also works in lists:

```bash
gbatch --depends-on-all @,@~1,@~2 python merge.py
```

## Conda Env Auto-detect

When submitting a command (not a script), if `--conda-env` is omitted, `gbatch` will use the current shell’s `$CONDA_DEFAULT_ENV` (if set).

Example:

```bash
conda activate myenv
gbatch python -c 'import os,sys; print("CONDA_DEFAULT_ENV=", os.getenv("CONDA_DEFAULT_ENV")); print("python=", sys.executable)'
```

Example output:
```
Submitted batch job 42 (silent-pump-6338)
```

Verify:

```bash
gjob show 42
cat ~/.local/share/gflow/logs/42.log
```

Example log output:
```
CONDA_DEFAULT_ENV= myenv
python= /path/to/miniconda/envs/myenv/bin/python
```

## Parameter Sweeps

Use `--param` to submit multiple jobs from one command by filling `{param}` placeholders (cartesian product across params).

```bash
gbatch --dry-run \
  --param lr=0.001,0.01 \
  --param bs=32,64 \
  --name-template 'lr{lr}_bs{bs}' \
  python train.py --lr {lr} --batch-size {bs}
```

Example output:
```
Would submit 4 batch job(s):
  [1] python train.py --lr 0.001 --batch-size 32 (GPUs: 0)
  [2] python train.py --lr 0.001 --batch-size 64 (GPUs: 0)
  [3] python train.py --lr 0.01 --batch-size 32 (GPUs: 0)
  [4] python train.py --lr 0.01 --batch-size 64 (GPUs: 0)
```

Ranges are supported (no commas): `start:stop` or `start:stop:step` (use `:step` for floats, e.g. `0:1:0.1`).

### From a CSV (`--param-file`)

```bash
gbatch --param-file params.csv --name-template 'run_{id}' python train.py --id {id}
```

`params.csv` must have a header row; each row is one job’s parameter set.

### Limit Concurrency (`--max-concurrent`)

```bash
gbatch --param lr=0.001,0.01 --max-concurrent 1 python train.py --lr {lr}
```

## Tree View When Debugging Pipelines

```bash
gqueue -t
```

## Preview Before Cancelling

```bash
gcancel --dry-run <job_id>
gcancel <job_id>
```

## Redo a Whole Chain

Fix the root cause, then redo the failed job and all dependent jobs:

```bash
gjob redo <job_id> --cascade
```

## Restrict GPUs at Runtime

Keep GPUs free for non-gflow workloads by restricting what gflow can allocate:

```bash
gctl set-gpus 0,2
gctl show-gpus
gctl set-gpus all
```

See also: [Configuration -> GPU Selection](./configuration#gpu-selection).

## GPU Reservations (`gctl reserve`)

Block out GPUs for a user/time window (e.g. a demo) so other users can't take them while the reservation is active.

```bash
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
gctl reserve list --active
gctl reserve list --timeline --range 48h
gctl reserve cancel <reservation_id>
```

`--start` supports ISO8601 (e.g. `2026-01-28T14:00:00Z`) or `YYYY-MM-DD HH:MM` (local time). Times must be on `:00` or `:30`; durations are multiples of 30 minutes.

## See Also

- [Job Submission](./job-submission)
- [Job Dependencies](./job-dependencies)
- [Time Limits](./time-limits)
- [Quick Reference](../reference/quick-reference)
