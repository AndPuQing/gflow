# gflow v0.4.17 Release Notes

gflow v0.4.17 expands native macOS support, improves scheduler persistence
performance, and updates the MCP implementation without changing its public
tool contract.

## Highlights

### Apple Silicon scheduling

On Apple Silicon, gflow now detects the platform automatically and exposes the
integrated GPU as a logical scheduling slot. CPU and GPU memory requests are
accounted against the same physical unified-memory pool, preventing a workload
from reserving more combined memory than the machine provides.

### Native macOS packages

The release pipeline now builds PyPI wheels for both Apple Silicon
(`aarch64-apple-darwin`) and Intel macOS (`x86_64-apple-darwin`). The Rust test
matrix also runs on macOS 14 in addition to Linux.

### Faster persistence snapshots

Journal snapshots now serialize job specifications and runtime state directly,
avoiding reconstruction of the legacy combined job representation. Older state
and journal files remain supported and require no migration.

### MCP v2

The MCP server now uses rmcp v2 and its current tool-router API. Existing gflow
MCP tool names, inputs, and output schemas are unchanged.

## Compatibility

v0.4.17 is intended as a drop-in patch upgrade from v0.4.16. It does not change
CLI commands, HTTP endpoints, MCP tools, or configuration formats. Existing
scheduler state is loaded through the backward-compatible reader.

After the release is published, upgrade the PyPI package with:

```bash
python -m pip install --upgrade runqd==0.4.17
```

## Known Limitations

- Apple Silicon is represented as one logical GPU because NVML does not expose
  per-device VRAM for the integrated GPU. Scheduling uses total system unified
  memory and the memory limits declared on each job.
- Native macOS wheels and Apple Silicon scheduling are new release paths. The
  release must not be tagged until the macOS CI matrix and wheel builds pass.
