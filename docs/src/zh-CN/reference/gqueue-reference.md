# gqueue 命令参考

完整的 `gqueue` 命令参考 - gflow 的任务队列监控工具。

## 概要

```bash
gqueue [OPTIONS]
```

## 描述

`gqueue` 显示 gflow 队列中任务的信息。它提供灵活的筛选、格式化和可视化选项，类似于 Slurm 的 `squeue` 命令。

## 选项

### 筛选选项

#### `--states <STATES>`, `-s <STATES>`

按状态筛选任务（逗号分隔列表）。

**有效状态**：
- `Queued` 或 `PD`：等待资源或依赖
- `Running` 或 `R`：正在执行
- `Finished` 或 `CD`：成功完成
- `Failed` 或 `F`：以错误退出
- `Cancelled` 或 `CA`：手动取消
- `Timeout` 或 `TO`：超过时间限制

**示例**：
```bash
# 仅显示运行中的任务
gqueue -s Running

# 显示运行中和队列中的任务
gqueue -s Running,Queued

# 显示所有终止状态
gqueue -s Finished,Failed,Cancelled,Timeout

# 使用短形式
gqueue -s R,PD
```

#### `--jobs <IDS>`, `-j <IDS>`

按任务 ID 筛选（逗号分隔列表或范围）。

**示例**：
```bash
# 单个任务
gqueue -j 42

# 多个任务
gqueue -j 1,2,3

# 范围（如果支持）
gqueue -j 1-10

# 混合
gqueue -j 1,5,10-15
```

#### `--names <NAMES>`, `-N <NAMES>`

按任务名称筛选（逗号分隔列表）。

**示例**：
```bash
# 单个名称
gqueue -N training-job

# 多个名称
gqueue -N "prep,train,eval"

# 模式匹配（取决于实现）
gqueue -N "train*"
```

### 显示选项

#### `--limit <NUM>`, `-n <NUM>`

限制显示的任务数量。

**行为**：
- 正数：显示前 N 个任务
- 负数：显示最后 N 个任务（默认：-10）
- 零：显示所有任务（与 `--all` 相同）

**示例**：
```bash
# 显示最后 10 个任务（默认）
gqueue

# 显示最后 20 个任务
gqueue -n 20

# 显示前 5 个任务
gqueue -n 5

# 显示所有任务
gqueue -n 0
```

#### `--all`, `-a`

显示所有任务（等同于 `-n 0`）。

**示例**：
```bash
gqueue -a
```

#### `--format <FIELDS>`, `-f <FIELDS>`

自定义输出格式（逗号分隔字段列表）。

**可用字段**：
- `JOBID`：任务 ID 号
- `NAME`：任务名称（tmux 会话名称）
- `ST`：任务状态（短形式：PD, R, CD, F, CA, TO）
- `STATE`：任务状态（长形式：Queued, Running, Finished 等）
- `TIME`：已用时间（HH:MM:SS）
- `TIMELIMIT`：时间限制（HH:MM:SS 或 UNLIMITED）
- `NODES`：请求的 GPU 数量
- `NODELIST`：分配的 GPU ID（或队列中任务的原因）
- `NODELIST(REASON)`：NODELIST 的别名
- `PRIORITY`：任务优先级（0-255）
- `DEPENDENCY`：此任务依赖的任务 ID
- `USER`：任务提交者的用户名

**默认格式**：
```bash
JOBID,NAME,ST,TIME,TIMELIMIT,NODES,NODELIST(REASON)
```

**示例**：
```bash
# 最小输出
gqueue -f JOBID,NAME,ST

# 时间聚焦视图
gqueue -f JOBID,TIME,TIMELIMIT

# 资源聚焦视图
gqueue -f JOBID,NODES,NODELIST,STATE

# 完整信息
gqueue -f JOBID,NAME,STATE,TIME,TIMELIMIT,NODES,NODELIST,PRIORITY,DEPENDENCY
```

#### `--group`, `-g`

按状态分组任务。

**示例**：
```bash
$ gqueue -g
RUNNING:
JOBID    NAME                ST    TIME         TIMELIMIT
1        train-resnet        R     00:15:23     04:00:00
2        train-vit           R     00:12:45     04:00:00

QUEUED:
JOBID    NAME                ST    TIME         TIMELIMIT
3        eval-models         PD    00:00:00     00:30:00

FINISHED:
JOBID    NAME                ST    TIME         TIMELIMIT
4        preprocess          CD    00:05:12     01:00:00
```

#### `--tree`, `-t`

以依赖关系树格式显示任务。

**示例**：
```bash
$ gqueue -t
JOBID    NAME           ST    TIME         TIMELIMIT
1        data-prep      CD    00:05:23     01:00:00
├─ 2     train-model-a  R     00:15:45     04:00:00
│  └─ 4  eval-a         PD    00:00:00     00:10:00
└─ 3     train-model-b  R     00:15:50     04:00:00
   └─ 5  eval-b         PD    00:00:00     00:10:00
```

**特性**：
- 显示父子关系
- 可视化工作流结构
- 优雅地检测和处理循环依赖
- 使用方框绘制字符的 ASCII 树

### 排序选项

#### `--sort <FIELD>`, `-r <FIELD>`

按字段排序任务。

**有效字段**：
- `id`：任务 ID（默认）
- `state`：任务状态
- `time`：开始时间
- `name`：任务名称
- `gpus`：GPU 数量
- `priority`：任务优先级

**示例**：
```bash
# 按优先级排序（高到低）
gqueue -r priority

# 按名称排序
gqueue -r name

# 按已用时间排序
gqueue -r time

# 按 GPU 数量排序
gqueue -r gpus
```

**注意**：排序与筛选和格式化选项配合使用。

### 全局选项

#### `--config <PATH>`

使用自定义配置文件（隐藏选项）。

**示例**：
```bash
gqueue --config /path/to/custom.toml
```

#### `--help`, `-h`

显示帮助消息。

```bash
$ gqueue --help
<!-- cmdrun gqueue --help -->
```

#### `--version`, `-V`

显示版本信息。

```bash
$ gqueue --version
<!-- cmdrun gqueue --version -->
```

## 输出格式

### 默认输出

```
JOBID    NAME                ST    TIME         TIMELIMIT    NODES    NODELIST(REASON)
1        silent-pump-6338    R     00:15:23     02:00:00     1        0
2        brave-river-1234    PD    00:00:00     04:00:00     2        (Resources)
3        gentle-wave-9876    CD    00:45:12     UNLIMITED    0        N/A
```

### 列描述

| 列 | 描述 | 示例 |
|-----|------|------|
| JOBID | 唯一任务标识符 | 42 |
| NAME | 任务运行名称（tmux 会话） | silent-pump-6338 |
| ST | 状态（短形式） | R, PD, CD, F, CA, TO |
| STATE | 状态（长形式） | Running, Queued, Finished |
| TIME | 已用时间 | 00:15:23 |
| TIMELIMIT | 最大运行时间 | 02:00:00, UNLIMITED |
| NODES | GPU 数量 | 0, 1, 2 |
| NODELIST(REASON) | GPU ID 或等待原因 | 0,1 或 (Resources) |
| PRIORITY | 任务优先级 | 10（默认） |
| DEPENDENCY | 父任务 ID | 5（或 N/A） |

### 状态代码

| 代码 | 完整状态 | 含义 |
|------|---------|------|
| PD | Queued | 等待资源或依赖 |
| R | Running | 正在执行 |
| CD | Finished | 成功完成 |
| F | Failed | 以非零状态退出 |
| CA | Cancelled | 手动取消 |
| TO | Timeout | 超过时间限制 |

### 时间格式

**已用时间和时间限制**：
- 格式：`HH:MM:SS` 或 `D-HH:MM:SS`（带天数）
- 示例：
  - `00:15:23`：15 分钟 23 秒
  - `02:30:00`：2 小时 30 分钟
  - `1-04:30:00`：1 天 4 小时 30 分钟
  - `UNLIMITED`：无时间限制

### 节点列表格式

**对于运行中的任务**：逗号分隔的 GPU ID
```
0,1,2
```

**对于队列中的任务**：等待原因
```
(Resources)
(Dependency: Job 5)
```

**对于非 GPU 任务**：N/A
```
N/A
```

## 示例

### 基本使用

```bash
# 查看最后 10 个任务
gqueue

# 查看所有任务
gqueue -a

# 查看最后 20 个任务
gqueue -n 20
```

### 筛选

```bash
# 仅显示运行中的任务
gqueue -s Running

# 显示运行中和队列中的任务
gqueue -s Running,Queued

# 显示特定任务
gqueue -j 42

# 显示多个任务
gqueue -j 40,41,42

# 按名称显示任务
gqueue -N "training-job"
```

### 自定义格式

```bash
# 最小视图
gqueue -f JOBID,NAME,ST

# 时间聚焦
gqueue -f JOBID,NAME,TIME,TIMELIMIT

# GPU 聚焦
gqueue -f JOBID,NAME,NODES,NODELIST

# 优先级队列视图
gqueue -f JOBID,NAME,PRIORITY,ST -r priority
```

### 可视化

```bash
# 按状态分组
gqueue -g

# 显示依赖关系树
gqueue -t

# 带筛选的树视图
gqueue -s Running,Queued -t
```

### 排序

```bash
# 按优先级排序（最高优先）
gqueue -r priority

# 按已用时间排序
gqueue -r time

# 按任务 ID 排序（默认）
gqueue -r id
```

### 监控

```bash
# 实时监控队列
watch -n 2 gqueue

# 监控运行中的任务
watch -n 2 'gqueue -s Running'

# 使用自定义格式监控
watch -n 2 'gqueue -f JOBID,NAME,TIME,TIMELIMIT'

# 监控依赖关系树
watch -n 2 'gqueue -t'
```

### 组合选项

```bash
# 运行中的 GPU 任务及详情
gqueue -s Running -f JOBID,NAME,NODES,NODELIST,TIME

# 最后 5 个已完成的任务
gqueue -s Finished -n 5 -r time

# 所有队列中的任务分组
gqueue -s Queued -g

# 高优先级任务优先
gqueue -r priority -n 20
```

## 常见模式

### 检查任务状态

```bash
# 任务 42 是否在运行？
gqueue -j 42 -f ST

# 我的任务状态如何？
gqueue -N "my-job-name" -f JOBID,ST
```

### 监控管道

```bash
# 查看工作流
gqueue -t

# 监控管道进度
watch -n 2 'gqueue -t'
```

### 查找卡住的任务

```bash
# 长时间队列中的任务
gqueue -s Queued -r time -f JOBID,NAME,TIME

# 为什么我的任务在队列中？
gqueue -j 42 -t
```

### 资源监控

```bash
# 哪些 GPU 在使用中？
gqueue -s Running -f JOBID,NAME,NODES,NODELIST

# 有多少任务在等待 GPU？
gqueue -s Queued -f JOBID,NODELIST
```

### 任务历史

```bash
# 最近完成
gqueue -s Finished -n 10 -r time

# 失败的任务
gqueue -s Failed -a

# 超时的任务
gqueue -s Timeout -a
```

## 集成示例

### Shell 脚本

```bash
#!/bin/bash
# 等待任务完成

JOB_ID=42

while true; do
    STATUS=$(gqueue -j $JOB_ID -f ST | tail -n 1)

    if [ "$STATUS" = "CD" ]; then
        echo "Job completed successfully!"
        break
    elif [ "$STATUS" = "F" ] || [ "$STATUS" = "TO" ]; then
        echo "Job failed or timed out!"
        exit 1
    fi

    sleep 5
done
```

### 管道监控

```bash
#!/bin/bash
# 监控管道进度

echo "=== Pipeline Status ==="
gqueue -j 1,2,3,4,5 -t

echo -e "\n=== Running Jobs ==="
gqueue -s Running -f JOBID,NAME,TIME,TIMELIMIT

echo -e "\n=== Queued Jobs ==="
gqueue -s Queued -f JOBID,NAME,NODELIST
```

### 资源仪表板

```bash
#!/bin/bash
# 简单的资源仪表板

clear
echo "╔════════════════════════════════════════╗"
echo "║         gflow Resource Dashboard       ║"
echo "╚════════════════════════════════════════╝"

echo -e "\n📊 Running Jobs:"
gqueue -s Running -f JOBID,NAME,NODES,NODELIST

echo -e "\n⏳ Queued Jobs:"
gqueue -s Queued -f JOBID,NAME,NODES,NODELIST

echo -e "\n✅ Recently Completed:"
gqueue -s Finished -n 5 -r time -f JOBID,NAME,TIME
```

### 任务统计

```bash
#!/bin/bash
# 任务统计

TOTAL=$(gqueue -a -f JOBID | tail -n +2 | wc -l)
RUNNING=$(gqueue -s Running -f JOBID | tail -n +2 | wc -l)
QUEUED=$(gqueue -s Queued -f JOBID | tail -n +2 | wc -l)
FINISHED=$(gqueue -s Finished -f JOBID | tail -n +2 | wc -l)
FAILED=$(gqueue -s Failed -f JOBID | tail -n +2 | wc -l)

echo "Total jobs: $TOTAL"
echo "Running: $RUNNING"
echo "Queued: $QUEUED"
echo "Finished: $FINISHED"
echo "Failed: $FAILED"
```

## 故障排除

### 空输出

**可能原因**：
1. 队列中没有任务
2. 所有任务都被状态/名称/ID 筛选器过滤掉
3. 守护进程未运行

**解决方案**：
```bash
# 检查守护进程
ginfo

# 查看所有任务
gqueue -a

# 移除筛选器
gqueue
```

### 格式化问题

**问题**：列不对齐

**解决方案**：终端太窄或列太多
```bash
# 使用更少的列
gqueue -f JOBID,NAME,ST

# 增加终端宽度
```

### 状态未更新

**问题**：任务状态似乎过时

**解决方案**：守护进程每 5 秒更新一次状态
```bash
# 等待几秒
sleep 5
gqueue

gflowd down
gflowd up
```

## 性能说明

- `gqueue` 即使有数千个任务也很快
- 按 ID 筛选比按名称筛选更快
- 树视图在依赖关系很深时可能很慢
- 默认限制（-10）保持输出可管理

## 参见

- [gbatch](./gbatch-reference) - 任务提交参考
- [gcancel](./gcancel-reference) - 任务取消参考
- [ginfo](./ginfo-reference) - 调度器检查参考
- [快速参考](./quick-reference) - 命令速查表
- [任务提交](../user-guide/job-submission) - 任务提交指南
- [任务依赖](../user-guide/job-dependencies) - 依赖管理
