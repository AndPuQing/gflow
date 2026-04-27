# AI Agent、MCP 与 Skill

`gflow` 可以作为本地 `stdio` MCP 服务器运行，让 `Claude Code`、`Codex`、`OpenCode` 这类 agent CLI 直接把调度器操作当作工具调用，而不是每次都手写 shell 命令。

在 agent 的 MCP 配置里，把下面这条命令作为服务启动命令：

```bash
gflow mcp serve
```

`gflow mcp serve` 是本地 `stdio` server 的启动命令。MCP 客户端通常会按配置的命令和参数，把这类 `stdio` server 作为本地子进程拉起。

在接入任意 agent CLI 之前，先确认本地调度器状态正常：

```bash
gflowd up
gflowd status
ginfo
```

- `gflowd` 需要先启动。
- 如果 `gflow` 不在 `PATH` 中，请改用绝对路径。
- 如果想确认 MCP 子命令可用，可以运行 `gflow mcp serve --help`。

## Agent 安全工作流

优先使用只读工具和预览工具，再修改调度器状态：

- 只读规划：`get_health`、`get_info`、`list_jobs`、`get_job`、`get_job_log`、`get_stats`、`list_reservations`、`get_queue_pressure`、`triage_job`。
- Dry-run 预览：调用 `submit_jobs` 前先用 `preview_submit_jobs`，调用 `update_job` 前先用 `preview_update_job`。
- 会修改状态的工具：`submit_jobs`、`update_job`、`redo_job`、`cancel_job`、`hold_job`、`release_job`。

除非用户已经明确要求执行对应操作，agent 在调用任何会修改状态的工具前都应该先确认。排查失败任务时，先调用 `triage_job`，让回复包含任务状态、运行时间、GPU 分配、最近日志证据和重试建议。

## Claude Code

推荐按用户级配置：

```bash
claude mcp add --scope user gflow -- gflow mcp serve
```

常用检查命令：

```bash
claude mcp list
claude mcp get gflow
```

说明：

- 如果你希望配置只在当前项目生效，可以把 `--scope user` 改成 `--scope project`。
- 如果 `gflow` 不在 `PATH` 中，改成绝对路径，例如 `-- /home/you/.local/bin/gflow mcp serve`。

`CLAUDE.md` 示例：

```md
# gflow workflow

- Use the `gflow` MCP server for queue, job, and log operations.
- Prefer read operations before mutating scheduler state.
- Use `preview_submit_jobs` or `preview_update_job` before creating or changing jobs.
- Use `triage_job` before retrying failed or timed-out jobs.
- Ask before submit, cancel, hold, release, or update unless the user already asked for it.
- When a job fails, summarize the key log lines before proposing a retry.
```

## Codex

最小配置：

```bash
codex mcp add gflow -- gflow mcp serve
```

查看当前配置：

```bash
codex mcp list
codex mcp get gflow
```

也可以直接写入 `~/.codex/config.toml`：

```toml
[mcp_servers.gflow]
command = "gflow"
args = ["mcp", "serve"]
```

说明：

- 如果 `gflow` 不在 `PATH` 中，把 `command` 改成绝对路径。

`AGENTS.md` 示例：

```md
## gflow

- Use the `gflow` MCP server for scheduler actions when available.
- Prefer read tools before writes.
- Preview submissions and updates before mutating scheduler state.
- Use `triage_job` and `get_queue_pressure` when explaining failures or queue delays.
- Confirm destructive job actions unless the user explicitly asked for them.
- Include job id, requested GPUs, and recent log evidence when reporting failures.
```

## OpenCode

OpenCode 通常直接在配置文件里声明 MCP。全局配置默认放在 `~/.config/opencode/opencode.json`，项目级配置可以放在仓库根目录的 `opencode.json`；两者也都支持 `JSONC`。

最小示例：

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "gflow": {
      "type": "local",
      "command": ["gflow", "mcp", "serve"],
      "enabled": true
    }
  }
}
```

查看连接状态：

```bash
opencode mcp list
```

说明：

- OpenCode 的本地 MCP 使用 `type: "local"` 和命令数组。
- 如果 `gflow` 不在 `PATH` 中，把 `command` 改成绝对路径数组。

## 常见问题

### 已经加了 MCP，但 agent 看不到 gflow 工具

优先检查：

```bash
gflowd status
ginfo
gflow mcp serve --help
```

常见原因：

- `gflowd` 没启动。
- agent 启动时的 `PATH` 里没有 `gflow`。
- 本地配置文件指向了错误的守护进程地址或端口。
- 如果你直接在 shell 里启动 `gflow mcp serve`，它会等待来自 MCP 客户端的 stdio 通信。

## 另见

- [配置](../user-guide/configuration)
- [任务提交](../user-guide/job-submission)
- [实用技巧](../user-guide/tips)
- [gflowd 参考](../reference/gflowd-reference)
