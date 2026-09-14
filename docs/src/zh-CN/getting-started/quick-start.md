# 快速入门

用下面几条命令跑通 gflow 的最小流程。

::: tip 开始前
先确认 `tmux` 已安装。没有的话，先看[安装](./installation)。
:::

## 可选：初始化配置

生成默认配置：

```shell
gflowd init
```

## 第 1 步：启动调度器

启动守护进程：

```shell
gflowd start
```

::: tip
`gflowd start` 会按 systemd user service（已安装时）→ tmux → 直接进程的顺序
自动选择托管方式，无需关心 daemon 由谁托管。
:::

::: warning
如果 `gflowd start` 失败，先检查 `tmux`（或先用 `gflowd service install`
安装 systemd user service）。
:::

检查状态：

```shell
gflowd status
```

从另一个终端验证：

```shell
ginfo
```

## 第 2 步：提交任务

```shell
gbatch echo 'Hello from gflow!'
```

## 第 3 步：查看队列与日志

```shell
gqueue
```

查看日志：

```shell
gjob log <job_id>
```

输出已经过清理：ANSI 转义序列被去除，进度条重绘折叠为最终一帧。持久日志
文件位于 `~/.local/share/gflow/logs/<job_id>.log`（或
`$XDG_DATA_HOME/gflow/logs/`）；`gjob log <job_id> --path` 会打印该路径，
`gjob log <job_id> --follow` 会持续输出。`--raw`、`--no-ansi` 以及旧的归档
尝试（`<job_id>.log.old.*`）见 [gjob 参考](../reference/gjob-reference)。

::: info
通常先用 `gqueue` 找到任务编号。
:::

## 第 4 步：停止调度器

```shell
gflowd stop
```

## 接下来

- [提交任务](../user-guide/job-submission)
- [时间限制](../user-guide/time-limits)
- [任务依赖](../user-guide/job-dependencies)
- [GPU 管理](../user-guide/gpu-management)
- [配置](../user-guide/configuration)
- [多用户使用](../user-guide/multi-user)
- [命令速查](../reference/quick-reference)
