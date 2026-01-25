# gflow Documentation

This directory contains the VitePress documentation for gflow.

## Development

```bash
# Install dependencies
bun install

# Start development server
bun run docs:dev

# Build for production
bun run docs:build

# Preview production build
bun run docs:preview
```

## Structure

- `src/` - Documentation source files (Markdown)
- `.vitepress/` - VitePress configuration and theme
- `public/` - Static assets (logo, images, etc.)

## Writing Conventions

- **Terminology (EN)**: use “job” (not “task”) unless referring to an “array task”.
- **术语（中文）**：统一使用“任务”；“数组任务”用于 job array 场景。
- **Daemon vs scheduler**: use “daemon” specifically for `gflowd`, “scheduler” for the system.
- **Code blocks**: commands in fenced blocks should not include a `$` prompt; use placeholders like `<job_id>`.
- **Placeholders**: prefer `<job_id>` (avoid `<id>`, `<ID>`, `<JOBID>`).
- **Internal links**: use Markdown links (not inline code paths).

## Deployment

Deployment is handled via GitHub Pages (see the repository settings / workflows).
