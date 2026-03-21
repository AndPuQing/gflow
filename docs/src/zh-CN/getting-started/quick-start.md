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
gflowd up
```

::: warning
如果 `gflowd up` 失败，先检查 `tmux`。
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

::: info
通常先用 `gqueue` 找到任务编号。
:::

## 第 4 步：停止调度器

```shell
gflowd down
```

## 接下来

- [提交任务](../user-guide/job-submission)
- [时间限制](../user-guide/time-limits)
- [任务依赖](../user-guide/job-dependencies)
- [GPU 管理](../user-guide/gpu-management)
- [配置](../user-guide/configuration)
- [命令速查](../reference/quick-reference)
