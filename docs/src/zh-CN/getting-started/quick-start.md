# 快速入门

本指南将在几分钟内带您跑通 gflow 的最小闭环。

## 1）启动调度器

启动守护进程（在 tmux 会话中运行）：

```shell
gflowd up
```

检查状态：

```shell
gflowd status
```

从另一个终端验证可访问性：

```shell
ginfo
```

## 2）提交一个任务

```shell
gbatch echo 'Hello from gflow!'
```

## 3）查看队列与日志

```shell
gqueue
```

查看输出：

```shell
gjob log <job_id>
```

## 4）停止调度器

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
