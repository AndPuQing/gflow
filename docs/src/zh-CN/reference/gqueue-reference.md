# gqueue 参考

`gqueue` 用于查看任务列表，支持筛选、格式化输出，以及树/分组视图。

## 用法

```bash
gqueue [options]
gqueue completion <shell>
```

## 常用示例

```bash
gqueue                               # 最近 10 个任务
gqueue -a                            # 所有任务
gqueue -s Running,Queued             # 按状态筛选
gqueue -j 12,13,14                   # 按任务 ID 筛选（逗号分隔）
gqueue -T                            # 仅显示有活跃 tmux 会话的任务
gqueue -t                            # 依赖树视图
gqueue -g                            # 按状态分组
```

## 输出格式

默认格式：

```text
JOBID,NAME,ST,TIME,NODES,NODELIST(REASON)
```

自定义格式：

```bash
gqueue -f JOBID,NAME,ST,TIMELIMIT,MEMORY,NODELIST(REASON)
```

`-f/--format` 支持的字段：

- `JOBID`
- `NAME`
- `ST`
- `TIME`
- `TIMELIMIT`
- `MEMORY`
- `NODES`（请求的 GPU 数量）
- `NODELIST(REASON)`（运行中：GPU 索引；排队/暂停/已取消：原因）
- `USER`

`gqueue -t` 示例输出：

```
JOBID  NAME   ST  TIME      NODES  NODELIST(REASON)
1      prep   CD  00:02:15  0      -
├─2    train  R   00:10:03  1      0
└─3    eval   PD  -         0      (WaitingForDependency)
```

## 选项

- `-n, --limit <N>`：显示前/后 N 个任务（默认：`-10`；`0`=全部）
- `-a, --all`：显示全部（等同 `-n 0`）
- `-c, --completed`：仅显示已完成任务
- `--since <when>`：显示自 `1h`、`2d`、`3w`、`today`、`yesterday` 或时间戳以来的任务
- `-r, --sort <field>`：`id`、`state`、`time`、`name`、`gpus`、`priority`
- `-s, --states <list>`：状态列表（如 `Queued,Running`）
- `-j, --jobs <list>`：任务 ID 列表（如 `1,2,3`）
- `-N, --names <list>`：任务名列表
- `-f, --format <fields>`：输出字段列表
- `-g, --group`：按状态分组
- `-t, --tree`：树视图（依赖 + redo 关系）
- `-T, --tmux`：仅显示有活跃 tmux 会话的任务
