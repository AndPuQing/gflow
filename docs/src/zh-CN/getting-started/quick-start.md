# 快速入门

本指南将在几分钟内带您跑通 gflow 的最小闭环。

::: tip 开始前
请先确认 `tmux` 已安装。若尚未安装，请先完成[安装](./installation)。
:::

## 可选：初始化配置

生成带有合理默认值的配置文件：

```shell
gflowd init
```

## 第 1 步：启动调度器

启动守护进程（在 tmux 会话中运行）：

```shell
gflowd up
```

::: warning
如果 `gflowd up` 启动失败，最常见的原因是系统中没有安装 `tmux`。
:::

检查状态：

```shell
gflowd status
```

从另一个终端验证可访问性：

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

查看输出：

```shell
gjob log <job_id>
```

::: info
通常先通过 `gqueue` 找到任务编号，再用 `gjob log <job_id>` 查看对应输出。
:::

## 第 4 步：停止调度器

```shell
gflowd down
```

## 下一步

- [提交任务](../user-guide/job-submission)
- [时间限制](../user-guide/time-limits)
- [任务依赖](../user-guide/job-dependencies)
- [GPU 管理](../user-guide/gpu-management)
- [配置](../user-guide/configuration)
- [命令速查](../reference/quick-reference)
