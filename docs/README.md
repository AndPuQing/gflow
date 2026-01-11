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

## Deployment

The documentation is automatically deployed to GitHub Pages via the `scripts/pages.sh` script.
