# 实用技巧

一些能让 gflow 工作流更快、更安全的小技巧。

## 依赖语法糖（`@`）

用 `@` 引用最近提交的任务：

- `@`：最近一次提交的任务
- `@~1`：上一次提交
- `@~2`：倒数第三次提交

```bash
gbatch --time 10 python preprocess.py
gbatch --depends-on @ --gpus 1 --time 4:00:00 python train.py
gbatch --depends-on @ --time 10 python evaluate.py
```

`@` 也可以用于列表：

```bash
gbatch --depends-on-all @,@~1,@~2 python merge.py
```

## Conda 环境自动探测

提交命令（非脚本）时，如果未指定 `--conda-env`，`gbatch` 会使用当前 shell 的 `$CONDA_DEFAULT_ENV`（如果存在）。

示例：

```bash
conda activate myenv
gbatch python -c 'import os,sys; print("CONDA_DEFAULT_ENV=", os.getenv("CONDA_DEFAULT_ENV")); print("python=", sys.executable)'
```

示例输出：
```
Submitted batch job 42 (silent-pump-6338)
```

验证：

```bash
gjob show 42
cat ~/.local/share/gflow/logs/42.log
```

示例日志输出：
```
CONDA_DEFAULT_ENV= myenv
python= /path/to/miniconda/envs/myenv/bin/python
```

## 参数搜索

用 `--param` 一次提交多个任务：在命令里用 `{param}` 占位符，并对多个参数做笛卡尔积展开。

```bash
gbatch --dry-run \
  --param lr=0.001,0.01 \
  --param bs=32,64 \
  --name-template 'lr{lr}_bs{bs}' \
  python train.py --lr {lr} --batch-size {bs}
```

示例输出：
```
Would submit 4 batch job(s):
  [1] python train.py --lr 0.001 --batch-size 32 (GPUs: 0)
  [2] python train.py --lr 0.001 --batch-size 64 (GPUs: 0)
  [3] python train.py --lr 0.01 --batch-size 32 (GPUs: 0)
  [4] python train.py --lr 0.01 --batch-size 64 (GPUs: 0)
```

支持范围写法（不含逗号）：`start:stop` 或 `start:stop:step`（浮点数建议带 `:step`，例如 `0:1:0.1`）。

### 从 CSV 读取（`--param-file`）

```bash
gbatch --param-file params.csv --name-template 'run_{id}' python train.py --id {id}
```

`params.csv` 必须有表头；每一行对应一个任务的参数集合。

### 限制并发（`--max-concurrent`）

```bash
gbatch --param lr=0.001,0.01 --max-concurrent 1 python train.py --lr {lr}
```

## 排查管道用树视图

```bash
gqueue -t
```

## 取消前先预览

```bash
gcancel --dry-run <job_id>
gcancel <job_id>
```

## 级联重做整条链

修复根因后，重做失败任务及其依赖任务：

```bash
gjob redo <job_id> --cascade
```

## 运行时限制可用 GPU

通过限制 gflow 允许分配的 GPU，给非 gflow 工作负载留出 GPU：

```bash
gctl set-gpus 0,2
gctl show-gpus
gctl set-gpus all
```

另见：[配置 -> GPU 选择](./configuration#gpu-selection)。

## GPU 预留（`gctl reserve`）

为某个用户在某段时间内预留 GPU（例如演示/会议）；预留生效期间，其他用户无法占用这部分 GPU。

```bash
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
gctl reserve list --active
gctl reserve list --timeline --range 48h
gctl reserve cancel <reservation_id>
```

`--start` 支持 ISO8601（例如 `2026-01-28T14:00:00Z`）或 `YYYY-MM-DD HH:MM`（本地时间）。开始时间分钟必须是 `00` 或 `30`；时长必须是 30 分钟的整数倍。

## 另见

- [任务提交](./job-submission)
- [任务依赖](./job-dependencies)
- [时间限制](./time-limits)
- [快速参考](../reference/quick-reference)
