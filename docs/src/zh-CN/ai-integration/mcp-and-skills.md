# AI Agent、MCP 与 Skill

`gflow` 可以作为本地 `stdio` MCP 服务器运行，让 `Claude Code`、`Codex`、`OpenCode` 这类 agent CLI 直接把调度器操作当作工具调用，而不是每次都手写 shell 命令。

```bash
gflow mcp serve
```

这类集成最适合“本机优先”的工作流：`gflowd` 持续运行在同一台 Linux 机器上，agent 通过本地配置连接守护进程。

## 先决条件

在接入任意 agent CLI 之前，先确认这三个命令都正常：

```bash
gflowd up
ginfo
gflow mcp serve
```

- `gflowd` 需要先启动。
- `gflow mcp serve` 是 `stdio` 服务器，不是 HTTP 服务。
- 如果 `gflow` 不在 `PATH` 中，请改用绝对路径。
- 推荐先让 `ginfo` 能正常返回，再交给 agent 使用 MCP。

## 该把配置放到哪里

| 客户端 | MCP 配置入口 | 长期指令 / Skill |
| --- | --- | --- |
| Claude Code | `claude mcp add ...` | 项目级指令建议放 `CLAUDE.md` |
| Codex | `codex mcp add ...` 或 `~/.codex/config.toml` | 项目级指令建议放 `AGENTS.md` |
| OpenCode | `opencode.json` / `opencode.jsonc` 的 `mcp` 字段 | 可复用能力建议写成 `SKILL.md` |

这三类东西解决的是不同问题：

- `MCP`：给 agent 提供“可执行工具”，例如查看队列、读日志、提交任务、取消任务。
- `AGENTS.md` / `CLAUDE.md`：给 agent 提供仓库级长期说明，例如“优先读后写”“提交任务前先确认 GPU 数量”。
- `SKILL.md`：把一组可复用工作流打包成技能，方便 agent 按需加载。

## 先看仓库里的现成示例

这个仓库本身已经提供了可直接参考的 skill 和 agent 配置：

- `skills/gflow-ops/SKILL.md`：`gflow` 运维类 skill 的主定义
- `skills/gflow-ops/agents/openai.yaml`：面向 OpenAI / Codex 风格 agent 的展示元数据
- `CLAUDE.md`：仓库级长期约束和工程规范

如果你要为 `gflow` 写 skill，优先参考这些文件的结构和语气，而不是从零开始写一个抽象模板。

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
- Claude Code 的仓库级持久化说明更适合放到项目根目录的 `CLAUDE.md`，而不是把大段流程提示反复写进每次对话。

`CLAUDE.md` 示例：

```md
# gflow workflow

- Use the `gflow` MCP server for queue, job, and log operations.
- Prefer read operations before mutating scheduler state.
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
- 如果你已经把仓库工作约束写进 `AGENTS.md`，Codex 往往不需要在 prompt 里重复这些固定规则。
- 对 Codex 来说，`AGENTS.md` 通常比“把所有团队规范都塞进一次性提示词”更稳定。

`AGENTS.md` 示例：

```md
## gflow

- Use the `gflow` MCP server for scheduler actions when available.
- Prefer read tools before writes.
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
- OpenCode 支持把与 gflow 相关的工作流写成可复用 `SKILL.md`，由 agent 按需加载。
- OpenCode 也能兼容发现 `.claude/skills` 和 `.agents/skills` 目录里的技能。

## Skill 怎么使用

直接用仓库自带的：

```text
skills/gflow-ops/
├── SKILL.md
└── agents/openai.yaml
```

建议：

- 优先复用 `skills/gflow-ops/SKILL.md`
- 需要定制时，复制后再改
- 不要先写一份新的通用模板

### 推荐放置位置

如果你的 agent 支持 `SKILL.md` 发现机制，可以放在这些位置之一：

- 项目级：`.opencode/skills/gflow-operator/SKILL.md`
- 全局：`~/.config/opencode/skills/gflow-operator/SKILL.md`
- Claude 兼容项目级：`.claude/skills/gflow-operator/SKILL.md`
- Claude 兼容全局：`~/.claude/skills/gflow-operator/SKILL.md`
- agent 兼容项目级：`.agents/skills/gflow-operator/SKILL.md`
- agent 兼容全局：`~/.agents/skills/gflow-operator/SKILL.md`

补充约束：

- 目录名应与 frontmatter 里的 `name` 一致。
- `name` 建议使用小写字母、数字和中划线，例如 `gflow-operator`。

说明文件分工：

- `CLAUDE.md`：适合 Claude Code
- `AGENTS.md`：适合 Codex
- `SKILL.md`：适合按需加载的专项流程

## 常见问题

### 已经加了 MCP，但 agent 看不到 gflow 工具

优先检查：

```bash
gflowd status
ginfo
gflow mcp serve
```

常见原因：

- `gflowd` 没启动。
- agent 启动时的 `PATH` 里没有 `gflow`。
- 本地配置文件指向了错误的守护进程地址或端口。

### 该把说明写进 Skill，还是写进仓库指令文件

经验上：高频规则放 `AGENTS.md` / `CLAUDE.md`，专项流程放 `SKILL.md`。

## 另见

- [配置](../user-guide/configuration)
- [任务提交](../user-guide/job-submission)
- [实用技巧](../user-guide/tips)
- [gflowd 参考](../reference/gflowd-reference)
