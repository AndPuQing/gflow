# gjob 参考

`gjob` 用于管理已有任务，包括查看详情、修改排队任务、重提任务，以及处理 tmux 会话。

## 用法

```bash
gjob <command> [args]
gjob completion <shell>
```

## 常见示例

```bash
# 查看任务详情
gjob show 42

# 输出最近一次任务的日志
gjob log @

# 只看前 20 行日志
gjob log @ --first 20

# 只看后 50 行日志
gjob log 42 --last 50

# 连接到正在运行任务的 tmux 会话
gjob attach @

# 暂停或恢复排队任务
gjob hold 10-12
gjob release 10,11

# 延时释放：把挂起的任务放回队列，但不早于指定时间开始
gjob release 12 --at now+1h
gjob release 12 --at 22:00

# 原地修改排队/暂停任务
gjob update 42 --gpus 2 --time-limit 4:00:00
gjob update 42 --max-retries 2

# 重提失败任务，并增加时间限制
gjob redo 42 --time 8:00:00

# 修复父任务后，级联重做被其失败连带取消的子任务
gjob redo 42 --cascade

# 清理已完成任务的 tmux 会话
gjob close-sessions --all
```

## 子命令

### `gjob attach <job>`

连接到任务的 tmux 会话。

别名：`gjob a`

```bash
gjob attach <job>
```

`<job>` 支持数字任务 ID，或用 `@` 表示最近一次任务。

### `gjob log <job>`

将任务日志输出到标准输出。

别名：`gjob l`

```bash
gjob log <job> [options]
```

`<job>` 支持数字任务 ID，或用 `@` 表示最近一次任务。

选项：

- `-f, --first <lines>`：只输出前 N 行
- `-l, --last <lines>`：只输出后 N 行

### `gjob hold <job_ids>`

将排队中的任务设为 hold。

别名：`gjob h`

```bash
gjob hold <job_ids>
```

`<job_ids>` 支持单个 ID、逗号分隔列表，以及 `1-3` 这样的区间。

### `gjob release <job_ids>`

将 hold 状态的任务重新放回队列。

别名：`gjob r`

```bash
gjob release <job_ids>
gjob release <job_ids> --at <time>
```

`<job_ids>` 支持单个 ID、逗号分隔列表，以及 `1-3` 这样的区间。

带 `--at <time>` 时任务会立即放回队列，但不会早于指定时间开始（延时释放）：
任务保持 Queued（原因 `BeginTime`），到点后才可被调度。`--at` 支持
`HH:MM[:SS]`、`YYYY-MM-DD[THH:MM[:SS]]`，或相对时间 `now+N[s|m|h|d]`（默认分钟）。

### `gjob show <job_ids>`

显示任务详细信息，包括资源、依赖、时间信息和 tmux 会话名。

别名：`gjob s`

```bash
gjob show <job_ids>
```

`<job_ids>` 支持单个 ID、逗号分隔列表，以及 `1-3` 这样的区间。

当任务发布了进度（见 `gjob progress`）时，会额外显示 `Progress:` 块，
包含已完成量、百分比、速率、ETA 和最后更新时间。`Log:` 块会报告日志文件的
路径、大小、最后修改时间和最后一行，因此可以区分「卡住的作业」和「只是较慢的
作业」。日志信息要求运行命令的机器能读取该日志文件。

```text
Progress:
  Value=24925/40000 (62.3%)
  Rate=0.52/s
  ETA=08:03:19 (at 09/14-22:12:04)
  Message=epoch 25/40
  Updated=09/14-14:08:45 (00:00:03 ago)
Log:
  Path=/home/alice/.local/share/gflow/logs/354.log
  Size=1.2M
  Modified=09/14-14:08:45 (last write 00:00:03 ago)
  LastLine=epoch 25/40 loss=0.412
```

### `gjob progress [<job>]`

为运行中的任务发布进度，使 `gjob show` 与 `gqueue` 能显示「进行到哪一步」
以及「预计何时完成」。gflow 无法自行推断这些信息，需要任务主动上报。

别名：`gjob p`

```bash
gjob progress <job> --value <units> [--total <units>] [-m <message>]
```

选项：

- `--value <units>`（必填）：已完成的工作单元数（step、epoch、样本等）
- `--total <units>`：总工作单元数；设置后才有百分比与 ETA
- `-m, --message <text>`：简短状态文本，例如 `epoch 25/40`
- `-s, --silent`：不输出任何内容；供频繁上报的任务使用

在 gflow 任务内部可以省略 `<job>`：executor 会导出 `GFLOW_JOB_ID`，因此训练脚本
直接调用 `gjob progress --silent --value 24925 --total 40000` 即可。

```bash
# 任务内部（任务 ID 来自 $GFLOW_JOB_ID）
gjob progress --value 24925 --total 40000 -m "epoch 25/40"

# 从外部为指定任务发布
gjob progress 354 --value 24925 --total 40000
```

对非运行中的任务发布会报错：进度只描述正在进行的作业。

#### 不依赖 `gjob` 的上报方式

无法调用 `gjob` 的任务可以直接写入同样的文档。executor 会导出
`GFLOW_PROGRESS_FILE`，即 daemon 读取的每任务路径：

```bash
cat > "$GFLOW_PROGRESS_FILE" <<EOF
{"value": 24925, "total": 40000, "message": "epoch 25/40"}
EOF
```

`value` 必填，`total` 与 `message` 可选。若希望速率从任务开始之后的某个时间点
计算（例如漫长的数据加载阶段之后），可写入可选的 `started_at_unix_secs`。
文档需小于 8 KiB；超过该大小、无法解析或缺少 `value` 的文档会被 daemon 忽略。

ETA 是按观测速率计算出的剩余工作量，而不是承诺：它不会扣除任务空转的时间，因此
暂停过的任务恢复后仍需要这些时间。

如果任务 15 分钟没有刷新文档，进度会被标记为 stale 而非丢弃——「只上报过一次
然后卡住」正是这个标记要暴露的情况。`gjob show` 会打印 `[STALE]`，并把 ETA 显示为
`<时间> (~, from stale progress)` 而不是预测的完成时刻；`gqueue -f PROGRESS` 会追加
`stale`，`-f ETA` 则带 `~` 后缀。

### `gjob update <job_ids>`

原地更新排队中或 hold 状态的任务。

别名：`gjob u`

```bash
gjob update <job_ids> [options]
```

选项：

- `-c, --command <command>`：替换命令
- `-s, --script <path>`：替换脚本路径
- `-g, --gpus <count>`：修改 GPU 数量
- `-e, --conda-env <name>`：设置 conda 环境
- `--clear-conda-env`：清除 conda 环境
- `-p, --priority <0-255>`：修改优先级
- `-t, --time-limit <time>`：修改时间限制
- `--clear-time-limit`：清除时间限制
- `-m, --memory-limit <memory>`：修改主机内存限制
- `--clear-memory-limit`：清除主机内存限制
- `--gpu-memory <memory>`：修改每张 GPU 的显存限制
- `--clear-gpu-memory-limit`：清除每张 GPU 的显存限制
- `-d, --depends-on <ids>`：替换依赖
- `--depends-on-all <ids>`：设置 AND 依赖
- `--depends-on-any <ids>`：设置 OR 依赖
- `--auto-cancel-on-dep-failure`：开启依赖失败自动取消
- `--no-auto-cancel-on-dep-failure`：关闭依赖失败自动取消
- `--max-concurrent <n>`：设置任务组最大并发
- `--clear-max-concurrent`：清除任务组最大并发
- `--max-retries <n>`：设置自动重试次数
- `--clear-max-retries`：清除自动重试次数
- `--param <key=value>`：更新模板参数，可重复传入

说明：

- 设置了自动重试次数后，`gjob show <job>` 会显示 `MaxRetries`。
- 自动重试当前只适用于执行失败；超时仍需要人工处理。

### `gjob redo <job>`

基于已有任务创建一个新任务，并可覆盖部分字段。

```bash
gjob redo <job> [options]
```

选项：

- `-g, --gpus <count>`：覆盖 GPU 数量
- `-p, --priority <0-255>`：覆盖优先级
- `-d, --depends-on <job|@>`：覆盖依赖
- `-t, --time <time>`：覆盖时间限制
- `-m, --memory <memory>`：覆盖主机内存限制
- `--gpu-memory <memory>`：覆盖每张 GPU 的显存限制
- `-e, --conda-env <name>`：覆盖 conda 环境
- `--clear-deps`：清除从原任务继承的依赖
- `--cascade`：同时重做因该任务失败而被自动取消的下游任务

`<job>` 支持数字任务 ID，或用 `@` 表示最近一次任务。

### `gjob close-sessions`

默认关闭已完成任务的 tmux 会话；也可以通过过滤条件精确指定。

别名：`gjob close`

```bash
gjob close-sessions [options]
```

选项：

- `-j, --jobs <job_ids>`：按任务 ID、区间或逗号分隔列表筛选
- `-s, --state <states>`：按状态筛选，例如 `finished,failed,cancelled`
- `-p, --pattern <text>`：按 tmux 会话名子串筛选
- `-a, --all`：关闭所有已完成任务的会话，但会跳过当前仍在运行任务的会话

说明：

- 不带任何过滤条件时，`gjob close-sessions` 不会执行关闭操作。
- 带过滤条件时，默认只会命中终态任务。
- 如需关闭非终态任务的会话，请显式传 `--state`。

### `gjob completion <shell>`

生成 shell 自动补全脚本。

```bash
gjob completion bash
gjob completion zsh
gjob completion fish
```

## 格式

- 时间值支持 `HH:MM:SS`、`MM:SS`，或单个整数表示分钟。
- 内存值支持 `512` 这类 MB 整数，或 `1024M`、`24G` 这类单位格式。
- GPU 显存值使用相同语法，并按每张 GPU 生效。

## 另见

- [任务提交](../user-guide/job-submission)
- [任务生命周期](../user-guide/job-lifecycle)
- [任务依赖](../user-guide/job-dependencies)
