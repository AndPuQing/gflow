# AI Agents, MCP, and Skills

`gflow` can run as a local `stdio` MCP server, allowing agent CLIs such as `Claude Code`, `Codex`, and `OpenCode` to treat scheduler operations as tool calls instead of rewriting shell commands every time.

Use this as the MCP server command in your agent configuration:

```bash
gflow mcp serve
```

`gflow mcp serve` is a local stdio server command. MCP clients typically launch stdio servers as local child processes using the configured command and arguments.

Before connecting any agent CLI, first make sure the local scheduler is healthy:

```bash
gflowd up
gflowd status
ginfo
```

- `gflowd` must be started first.
- If `gflow` is not on `PATH`, use an absolute path instead.
- To confirm the MCP subcommand is available, run `gflow mcp serve --help`.

## Agent-Safe Workflow

Use read-only and preview tools before mutating scheduler state:

- Read-only planning: `get_health`, `get_info`, `list_jobs`, `get_job`, `get_job_log`, `get_stats`, `list_reservations`, `get_queue_pressure`, `triage_job`.
- Dry-run previews: `preview_submit_jobs` before `submit_jobs`, and `preview_update_job` before `update_job`.
- Mutating tools: `submit_jobs`, `update_job`, `redo_job`, `cancel_job`, `hold_job`, `release_job`.

Agents should ask for explicit confirmation before calling any mutating tool unless the user has already requested that exact action. For failures, call `triage_job` first so the response includes job state, runtime, GPU assignment, recent log evidence, and retry hints.

## Claude Code

User-scope configuration is recommended:

```bash
claude mcp add --scope user gflow -- gflow mcp serve
```

Common check commands:

```bash
claude mcp list
claude mcp get gflow
```

Notes:

- If you want the configuration to apply only to the current project, change `--scope user` to `--scope project`.
- If `gflow` is not on `PATH`, switch to an absolute path, for example `-- /home/you/.local/bin/gflow mcp serve`.

Example `CLAUDE.md`:

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

Minimal configuration:

```bash
codex mcp add gflow -- gflow mcp serve
```

View the current configuration:

```bash
codex mcp list
codex mcp get gflow
```

You can also write it directly to `~/.codex/config.toml`:

```toml
[mcp_servers.gflow]
command = "gflow"
args = ["mcp", "serve"]
```

Notes:

- If `gflow` is not on `PATH`, change `command` to an absolute path.

Example `AGENTS.md`:

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

OpenCode usually declares MCP directly in its config file. The global config is typically at `~/.config/opencode/opencode.json`, and a project-level config can be placed at `opencode.json` in the repository root. Both also support `JSONC`.

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

Check connection status:

```bash
opencode mcp list
```

Notes:

- OpenCode local MCP uses `type: "local"` and a command array.
- If `gflow` is not on `PATH`, change `command` to an absolute-path array.

## FAQ

### MCP is already added, but the agent cannot see the gflow tools

Check these first:

```bash
gflowd status
ginfo
gflow mcp serve --help
```

Common causes:

- `gflowd` is not running.
- `gflow` is not on the `PATH` seen by the agent process.
- The local config points to the wrong daemon address or port.
- If you start `gflow mcp serve` directly in a shell, it will wait for stdio traffic from an MCP client.

## See Also

- [Configuration](../user-guide/configuration)
- [Job Submission](../user-guide/job-submission)
- [Tips](../user-guide/tips)
- [gflowd Reference](../reference/gflowd-reference)
