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

日志文件是 daemon 写入的持久捕获。使用 `tmux` 执行器时它是原始的 pane
捕获，因此会包含 shell 提示符、ANSI 颜色与光标转义序列，以及进度条用
回车符重绘的内容（一个 `tqdm` 进度条可能在同一行出现数百次）。

默认情况下 `gjob log` 会清理这些内容：去掉转义序列，并把每次回车重绘
折叠为最终一帧，输出即为可读的纯文本，每次更新只占一行。

选项：

- `-f, --first <lines>`：只输出前 N 行
- `-l, --last <lines>`：只输出后 N 行
- `--raw`：原样输出 tmux 捕获（保留转义序列与 `\r`）
- `--no-ansi`：去掉转义序列，但每次重绘各占一行
- `--path`：只打印日志文件路径，不读取内容（便于脚本使用）
- `-F, --follow`：持续输出追加内容，直到任务结束

`--first`/`--last` 按渲染后的行数计算，而不是原始物理行数，因此对含大量
进度条的日志切片也符合预期。（使用 `--raw` 时按原始物理行切片。）`--follow`
不能与切片选项或 `--path` 同时使用。当日志尚未生成时，`--path` 与 `--follow`
会提示日志不可用。

#### 日志文件位置

任务 `<jobid>` 的持久日志位于 gflow 数据目录：

```
$XDG_DATA_HOME/gflow/logs/<jobid>.log      # 默认：~/.local/share/gflow/logs/<jobid>.log
```

被取代的旧尝试（例如 `gjob redo` 或自动重试之后）会归档为同目录下的
`<jobid>.log.old.<timestamp>`。当 `gjob log` 不可用时可以直接读取该文件：

```bash
# 只取路径，不读取文件（便于脚本使用）
gjob log 42 --path

# 等价于对持久日志执行 `tail -f`
gjob log 42 --follow

# 只保留最后一个已完成的进度帧
gjob log 42 | grep -E '^step=' | tail -1
```

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
