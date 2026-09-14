# gqueue 参考

`gqueue` 用于查看任务列表，支持筛选、格式化输出，以及树/分组视图。

## 用法

```bash
gqueue [options]
gqueue completion <shell>
```

## 常用示例

```bash
gqueue                               # 当前用户的活跃任务（Queued、Hold、Running）
gqueue -a                            # 所有任务（包含已完成任务）
gqueue -s Running,Queued             # 按状态筛选
gqueue -j 12,13,14                   # 按任务 ID 筛选（逗号分隔）
gqueue -u alice                      # 按用户筛选（默认当前用户；用 'all' 表示所有用户）
gqueue -P ml-research                # 按项目编码筛选
gqueue -T                            # 仅显示有活跃 tmux 会话的任务
gqueue -t                            # 依赖树视图
gqueue -g                            # 按状态分组
gqueue -w                            # 每 2 秒自动刷新
gqueue -w --interval 5               # 每 5 秒自动刷新
```

## 输出格式

默认格式：

```text
JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
```

自定义格式：

```bash
gqueue -f JOBID,NAME,PROJECT,ST,TIMELIMIT,MEMORY,NODELIST(REASON)
```

`-f/--format` 支持的字段：

- `JOBID`
- `NAME`
- `ST`
- `TIME`
- `TIMELIMIT`（实际生效的时间限制，未设置时为 `UNLIMITED`）
- `PROGRESS`（任务上报的进度：`已完成/总数 (百分比)`；没有总数时只显示已完成量；未上报时显示 `-`；停止刷新后追加 `stale`）
- `PERCENT`（完成百分比，未知时为 `-`）
- `ETA`（按观测速率计算的剩余工作量，不扣除任务空转时间，因此只是估算；到达总数时显示 `done`，未知时为 `-`；由 stale 进度估算出的值带 `~` 后缀）
- `MEMORY`
- `NODES`（请求的 GPU 数量）
- `NODELIST(REASON)`（运行中：GPU 索引；排队/暂停/已取消：原因）
- `USER`
- `PROJECT`
- `COMMAND`（作业运行的内容：命令提交显示存储的命令，脚本提交显示脚本路径——两者同时存在时 script 优先，与执行器一致；都没有则显示 `-`）

标准输出为终端时表格自动适配终端宽度（最宽列优先截断并加 `…`）；重定向到文件或管道时保留完整内容。`-f` 中任意字段可加 `:宽度` 后缀限制列宽（`:0` 显示完整），指定后跳过自适应。

任务可以自行上报进度（见 [`gjob progress`](./gjob-reference)），表格会显示该进度：

```bash
gqueue -f JOBID,NAME,ST,TIME,PROGRESS,PERCENT,ETA
```

```text
 JOBID   NAME         ST   TIME       PROGRESS       PERCENT   ETA
 354     infonce40k   R    08:57:37   24925/40000    62.3%     08:03:19
```

未上报进度的任务（默认情况）在 `PROGRESS`、`PERCENT`、`ETA` 列显示 `-`。停止刷新
进度的任务会在 `PROGRESS` 中标记 `stale`，其 `ETA` 带 `~` 后缀，因为该估算
来自已过期的测量值。

`gqueue -t` 示例输出：

```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

## 选项

- `-n, --limit <N>`：显示前/后 N 个任务（正数：前 N 个；负数：后 N 个；`0`：全部；默认：`0`）
- `-a, --all`：显示所有任务，包括已完成任务
- `-c, --completed`：仅显示已完成任务
- `--since <when>`：显示自 `1h`、`2d`、`3w`、`today`、`yesterday` 或时间戳以来的任务
- `-r, --sort <field>`：`id`、`state`、`time`、`name`、`gpus`、`priority`
- `-s, --states <list>`：状态列表（如 `Queued,Running`）
- `-u, --user <list>`：用户列表（默认当前用户；用 `all` 表示所有用户；别名：`--users`）
- `-j, --jobs <list>`：任务 ID 列表（如 `1,2,3`；别名：`--job`）
- `-N, --names <list>`：任务名列表
- `-P, --project <code>`：按项目编码筛选
- `-f, --format <fields>`：输出字段列表
- `-g, --group`：按状态分组
- `-t, --tree`：树视图（依赖 + redo 关系）
- `-T, --tmux`：仅显示有活跃 tmux 会话的任务
- `-w, --watch`：自动刷新任务列表（默认每 2 秒）
- `--interval <N>`：`--watch` 模式的刷新间隔（秒，默认：`2`）
