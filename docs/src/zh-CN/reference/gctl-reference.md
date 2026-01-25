# gctl 参考

`gctl` 是用于在运行时调整调度器行为的控制/管理命令行工具。

## 用法

```bash
gctl <command> [args]
```

## 子命令

### `gctl show-gpus`

显示当前 GPU 状态，以及它是否被调度器配置限制为不可用。

**输出格式**：

- 允许使用的 GPU：`<index>\t<available|in_use>`
- 被限制的 GPU：`<index>\t<available|in_use>\trestricted`

示例：

```bash
gctl show-gpus
```

### `gctl set-gpus <gpu_spec>`

限制调度器为新任务分配 GPU 的范围。

`<gpu_spec>` 支持：

- `all`（取消限制；允许所有检测到的 GPU）
- 逗号分隔：`0,2,4`
- 范围：`0-3`
- 混合：`0-1,3,5-6`

示例：

```bash
gctl set-gpus 0,2
gctl set-gpus 0-3
gctl set-gpus all
```

说明：

- 仅影响新的资源分配；运行中的任务不受影响。
- 等价于在 `~/.config/gflow/gflow.toml` 中配置 `daemon.gpus`。

### `gctl set-limit <job_or_group_id> <limit>`

设置某个任务组的最大并发数。

`<job_or_group_id>` 可以是：

- 任务 ID（该组中的任意一个任务）；`gctl` 会自动解析其 `group_id`
- 组 ID（UUID）

示例：

```bash
# 使用任务组内任意任务的 ID
gctl set-limit <job_id> 2

# 使用 group_id
gctl set-limit <group_id> 2
```

提示：当使用 `gbatch --max-concurrent` 提交参数批任务时，会创建任务组（group）。

### `gctl completion <shell>`

生成 Shell 补全脚本。

```bash
gctl completion bash
gctl completion zsh
gctl completion fish
```
