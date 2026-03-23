# 多用户使用

gflow 的多用户使用场景是：一台机器上由多个 Unix 用户共享同一个 `gflowd`，每个实际使用者都有自己的操作系统账号。

`gbatch` 会从当前 shell 环境变量（`USER` 或 `USERNAME`）记录任务提交者。默认情况下，`gqueue` 和 `gstats` 只显示当前用户的任务，所以即使调度器是共享的，日常使用体验仍然是“按用户隔离”的。

::: warning 安全模型
`gflowd` 暴露的是 HTTP API，目前没有内建认证或 RBAC。只要某个人能够访问守护进程地址，就可以查询调度状态并发送修改状态的请求。

因此在多用户部署时，优先让守护进程监听 `localhost`；如果必须开放到网络，请额外使用 SSH、防火墙、VPN 或带认证的代理来限制访问。
:::

## 推荐部署方式

1. 在这台机器上只运行一个共享 `gflowd`。
2. 每个人使用独立的 Unix 账号，不要共用同一个登录账号。
3. 所有 CLI 都连接到同一个本机守护进程地址和端口。
4. 将 `gflowd` 和 `gctl` 这类运维操作视为管理员专用流程。
5. 用项目标签和预约机制协调团队共享资源。

## 给管理员

### 只运行一个共享守护进程

整台机器只保留一个调度器，而不是每个用户各起一个 `gflowd`。

```toml
[daemon]
host = "localhost"
port = 59000
```

- `localhost` 是共享机器上最安全的默认值。
- 只有在你已经规划好网络边界时，才使用非 localhost 的监听地址。
- `gflowd` 的状态文件和日志文件保存在运行该守护进程的账号家目录下，见[配置](./configuration#文件与状态)。

### 统一本机入口

所有用户都应连接到同一个本机守护进程。默认配置就是 `localhost:59000`，通常不需要额外设置。

### 在 gflow 之外落实管理员边界

由于 gflow 目前没有内建角色系统，管理员和普通用户的边界需要通过操作系统权限和网络边界来落实：

- 只有受信任的用户才能访问守护进程地址。
- 只有管理员负责守护进程生命周期管理（`gflowd up`、`gflowd down`、`gflowd restart`）。
- `gctl set-gpus` 以及面向全团队的预约操作应只由管理员执行。
- 不要直接把 `gflowd` 暴露在 `0.0.0.0` 上，除非你同时加了额外的访问控制。

### 统一团队元数据

如果多个团队共享同一个调度器，建议要求所有任务都带项目标签：

```toml
[projects]
known_projects = ["ml-research", "cv-team"]
require_project = true
```

这样用户提交时就会变成：

```bash
gbatch --project ml-research python train.py
```

这会让 `gqueue --project ...`、`gstats` 和通知能力更有价值。

### 协调紧张的 GPU 资源

当你需要保护部分 GPU 容量时，可以使用运行时限制和预约：

```bash
gctl set-gpus 0-3
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
gctl reserve list --active
```

预约特别适合演示、答辩、截止日前冲刺这类需要时间窗口保障的场景。

### 查看全局使用情况

管理员常用视图：

```bash
gqueue -u all
gstats --user alice
gctl reserve list --timeline --range 48h
```

如果需要对接外部监控或审计，可以再配合[通知](./notifications)能力。

## 给普通用户

### 使用默认本机连接

### 以自己的身份提交任务

- 任务归属来自当前 shell 环境变量。
- 在共享调度器场景下，不要手动覆盖 `USER` 或 `USERNAME`。
- 每个人都应使用自己的 Unix 账号，而不是共用账号。

### 使用默认的“只看自己”视图

最常用的概览命令默认就只看当前用户：

```bash
gqueue
gstats
```

如果你要查看自己的任务编号，通常先运行一次 `gqueue`。

如果团队启用了项目标签，请始终保持一致：

```bash
gbatch --project ml-research python train.py
gqueue --project ml-research
```

### 遵守共享资源规则

- 如果管理员开启了必填项目，请始终传 `--project`。
- 如果 GPU 已经被其他用户预约，或者管理员通过 `gctl set-gpus` 收紧了可分配范围，你的任务可能会继续排队等待。
- 涉及全团队排期变更时，先联系管理员，不要自行执行预约类运维操作。

### 这些情况应联系管理员

出现以下情况时，应找管理员处理：

- 无法连接共享守护进程；
- 需要为演示或截止日期申请独占 GPU 时间窗口；
- 任务因预约或运行时 GPU 限制长期阻塞；
- 团队想调整共享项目策略或通知策略。

## 常见模式

### 共享工作站

- 所有人都登录到同一台机器。
- `gflowd` 监听在 `localhost`。
- 每个人都用自己的 Unix 账号运行 `gbatch`、`gqueue` 和 `gjob`。

## 另见

- [配置](./configuration)
- [任务提交](./job-submission)
- [实用技巧](./tips)
- [gctl 参考](../reference/gctl-reference)
- [gqueue 参考](../reference/gqueue-reference)
