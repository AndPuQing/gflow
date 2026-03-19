# AI Agents, MCP, and Skills

`gflow` can run as a local `stdio` MCP server so agent CLIs such as `Claude Code`, `Codex`, and `OpenCode` can treat scheduler operations as tools instead of relying on ad hoc shell wrappers.

```bash
gflow mcp serve
```

This works best in a local-first setup: keep `gflowd` running on the same Linux machine, then let the agent connect through local config.

## Prerequisites

Before configuring any agent CLI, make sure these all work:

```bash
gflowd up
ginfo
gflow mcp serve
```

- `gflowd` must already be running.
- `gflow mcp serve` is a `stdio` server, not an HTTP service.
- If `gflow` is not on `PATH`, use an absolute binary path.
- It is better to confirm `ginfo` works first, then wire the agent to MCP.

## Where each piece belongs

| Client | MCP entry point | Persistent instructions / skill |
| --- | --- | --- |
| Claude Code | `claude mcp add ...` | Project rules usually belong in `CLAUDE.md` |
| Codex | `codex mcp add ...` or `~/.codex/config.toml` | Project rules usually belong in `AGENTS.md` |
| OpenCode | `mcp` in `opencode.json` / `opencode.jsonc` | Reusable workflows are a good fit for `SKILL.md` |

These solve different problems:

- `MCP`: executable tools such as queue inspection, log reads, submit, update, hold, release, and cancel.
- `AGENTS.md` / `CLAUDE.md`: repo-level standing instructions such as "read before write" or "confirm before destructive job actions".
- `SKILL.md`: reusable workflows that the agent can load on demand.

## Start with the examples already in this repo

This repository already ships with concrete files you can reference:

- `skills/gflow-ops/SKILL.md`: the main gflow operations skill
- `skills/gflow-ops/agents/openai.yaml`: display metadata for OpenAI / Codex-style agents
- `CLAUDE.md`: standing repo-level guidance and engineering rules

If you are writing a gflow-related skill, start from these files instead of inventing a generic template from scratch.

## Claude Code

Recommended user-scope setup:

```bash
claude mcp add --scope user gflow -- gflow mcp serve
```

Useful checks:

```bash
claude mcp list
claude mcp get gflow
```

Notes:

- Use `--scope project` if you want the MCP entry to apply only inside the current project.
- If `gflow` is not on `PATH`, switch to an absolute path such as `-- /home/you/.local/bin/gflow mcp serve`.
- For repo-level guidance, Claude Code is usually better served by a root-level `CLAUDE.md` than by repeating the same instructions in every prompt.

Example `CLAUDE.md`:

```md
# gflow workflow

- Use the `gflow` MCP server for queue, job, and log operations.
- Prefer read operations before mutating scheduler state.
- Ask before submit, cancel, hold, release, or update unless the user already asked for it.
- When a job fails, summarize the key log lines before proposing a retry.
```

## Codex

Minimal setup:

```bash
codex mcp add gflow -- gflow mcp serve
```

Inspect the config:

```bash
codex mcp list
codex mcp get gflow
```

You can also configure it directly in `~/.codex/config.toml`:

```toml
[mcp_servers.gflow]
command = "gflow"
args = ["mcp", "serve"]
```

Notes:

- If `gflow` is not on `PATH`, replace `command` with an absolute path.
- For Codex, repo guidance is usually more stable in `AGENTS.md` than in long one-off prompts.
- If your repo already carries an `AGENTS.md`, keep gflow workflow rules there instead of duplicating them per session.

Example `AGENTS.md`:

```md
## gflow

- Use the `gflow` MCP server for scheduler actions when available.
- Prefer read tools before writes.
- Confirm destructive job actions unless the user explicitly asked for them.
- Include job id, requested GPUs, and recent log evidence when reporting failures.
```

## OpenCode

OpenCode usually declares MCP servers in config. The default global config lives at `~/.config/opencode/opencode.json`, and a project config can live at `opencode.json` in the repo root. Both JSON and JSONC are supported.

Minimal example:

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

Check status with:

```bash
opencode mcp list
```

Notes:

- Local MCP servers use `type: "local"` plus a command array.
- If `gflow` is not on `PATH`, switch `command` to an absolute-path array.
- OpenCode is a good fit for packaging gflow workflows as reusable `SKILL.md` definitions.
- OpenCode can also discover Claude-compatible and agent-compatible skill directories.

## How to use the existing skill

Use the built-in repo example:

```text
skills/gflow-ops/
├── SKILL.md
└── agents/openai.yaml
```

Recommended:

- reuse `skills/gflow-ops/SKILL.md`
- copy it only if you need customization
- do not start from a generic blank template

### Recommended locations

If your agent supports `SKILL.md` discovery, place it in one of these locations:

- Project: `.opencode/skills/gflow-operator/SKILL.md`
- Global: `~/.config/opencode/skills/gflow-operator/SKILL.md`
- Claude-compatible project: `.claude/skills/gflow-operator/SKILL.md`
- Claude-compatible global: `~/.claude/skills/gflow-operator/SKILL.md`
- Agent-compatible project: `.agents/skills/gflow-operator/SKILL.md`
- Agent-compatible global: `~/.agents/skills/gflow-operator/SKILL.md`

Additional constraints:

- The directory name should match the `name` in frontmatter.
- Use lowercase letters, digits, and hyphens for `name`, for example `gflow-operator`.

Use:

- `CLAUDE.md` for Claude Code
- `AGENTS.md` for Codex
- `SKILL.md` for on-demand workflows

## Troubleshooting

### The MCP entry exists, but the agent cannot see gflow tools

Check these first:

```bash
gflowd status
ginfo
gflow mcp serve
```

Common causes:

- `gflowd` is not running.
- The agent process cannot resolve `gflow` from `PATH`.
- Your local config points to the wrong daemon host or port.

### Should this go in a skill or in repo instructions?

In practice: frequent rules go in `AGENTS.md` / `CLAUDE.md`; specialized workflows go in `SKILL.md`.

## See Also

- [Configuration](../user-guide/configuration)
- [Job Submission](../user-guide/job-submission)
- [Tips](../user-guide/tips)
- [gflowd Reference](../reference/gflowd-reference)
