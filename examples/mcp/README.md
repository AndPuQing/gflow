# MCP Examples

`gflow` ships with a local stdio MCP server:

```bash
gflow mcp serve
```

Use that command as the MCP server entry in your client configuration. This mode is intended for local-first usage. Keep `gflowd` running on the same machine, then point your MCP client at `gflow mcp serve`. MCP clients typically launch local stdio servers using the configured command and arguments.

## Claude Desktop

Use [claude-desktop.json](./claude-desktop.json) as the server entry:

```json
{
  "mcpServers": {
    "gflow": {
      "command": "gflow",
      "args": ["mcp", "serve"]
    }
  }
}
```

## Notes

- Start `gflowd` first.
- `gflow mcp serve` is a stdio server, not an HTTP service.
- To confirm that the subcommand is available, run `gflow mcp serve --help`.
- The MCP tools are designed around local scheduler operations such as health checks, job inspection, log reads, submit, update, hold, release, and cancel.
