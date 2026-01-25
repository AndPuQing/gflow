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

