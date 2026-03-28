# gctl 参考

`gctl` 用于在运行时调整调度器行为。

## 用法

```bash
gctl <command> [args]
gctl completion <shell>
```

## 命令

### `gctl show-gpus`

查看每张 GPU 的状态（包含是否被限制）。

```bash
gctl show-gpus
```

### `gctl gpu-process ignore --gpu <index> --pid <pid>`

在 gflow 判断某张 GPU 是否被非托管进程占用时，忽略其中一个正在运行的 GPU 进程。

这是仅在运行时生效的 override。`gflowd` 重启或 reload 后会自动清空。

```bash
gctl gpu-process ignore --gpu 0 --pid 1234
```

### `gctl gpu-process unignore --gpu <index> --pid <pid>`

移除一条运行时 GPU 进程忽略规则。

```bash
gctl gpu-process unignore --gpu 0 --pid 1234
```

### `gctl gpu-process list`

列出当前生效的运行时 GPU 进程忽略规则。

```bash
gctl gpu-process list
```

### `gctl set-gpus <gpu_spec>`

限制调度器允许分配的 GPU（只影响**新的**分配）。

`<gpu_spec>` 示例：

- `all`
- `0,2,4`
- `0-3`
- `0-1,3,5-6`

```bash
gctl set-gpus 0,2
gctl set-gpus all
```

### `gctl set-limit <job_or_group_id> <limit>`

设置任务组的最大并发数。

```bash
gctl set-limit <job_id> 2
gctl set-limit <group_id> 2
```

### `gctl reserve create`

创建 GPU 预留并绑定到指定用户。

**按 GPU 数量**（调度器动态分配）：
```bash
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
```

**按具体 GPU 索引**（预留指定 GPU）：
```bash
gctl reserve create --user alice --gpu-spec 0,2 --start '2026-01-28 14:00' --duration 2h
gctl reserve create --user bob --gpu-spec 0-3 --start '2026-01-28 16:00' --duration 1h
```

`--start` 支持 ISO8601（例如 `2026-01-28T14:00:00Z`）或 `YYYY-MM-DD HH:MM`（本地时间）。开始时间分钟必须是 `00` 或 `30`；时长必须是 30 分钟的整数倍。

### `gctl reserve list`

列出预留记录。

```bash
gctl reserve list
gctl reserve list --active
gctl reserve list --user alice --status active
gctl reserve list --timeline --range 48h
```

### `gctl reserve get <reservation_id>`

查看某条预留的详细信息。

```bash
gctl reserve get <reservation_id>
```

### `gctl reserve cancel <reservation_id>`

取消预留。

```bash
gctl reserve cancel <reservation_id>
```
