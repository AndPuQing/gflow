# Installation

Install gflow with the steps below.

::: info Package Name
The Python package name is `runqd`. It provides `gflowd`, `gbatch`, `gqueue`, `gjob`, `ginfo`, `gcancel`, and `gctl`.
:::

## Prerequisites

- **Operating System**: Linux
- **tmux**: Required
- **NVIDIA GPU / drivers**: Only required for GPU scheduling

### Installing Prerequisites

::: code-group
```bash [Ubuntu/Debian]
# Install tmux
sudo apt-get update
sudo apt-get install tmux
```

```bash [Fedora/RHEL]
# Install tmux
sudo dnf install tmux
```
:::

## Installation Methods

### Method 1: Install via PyPI (Recommended)

Use `uv` or `pipx` for CLI installs:

::: code-group
```bash [uv]
uv tool install runqd
```

```bash [pipx]
pipx install runqd
```

```bash [pip]
pip install runqd
```
:::

Pre-built binaries are available for Linux `x86_64` and `ARM64`.

### Install Nightly Build

For the latest development build, install from TestPyPI:

::: code-group
```bash [uv]
uv tool install --index https://test.pypi.org/simple/ runqd
```

```bash [pipx]
pipx install --index-url https://test.pypi.org/simple/ runqd
```

```bash [pip]
pip install --index-url https://test.pypi.org/simple/ runqd
```
:::

### Method 2: Install via Cargo

Build and install from crates.io:

::: code-group
```bash [crates.io]
cargo install gflow
```

```bash [main branch]
cargo install --git https://github.com/AndPuQing/gflow.git --locked
```
:::

This installs binaries to `~/.cargo/bin/`. Make sure it is in your `PATH`.

### Method 3: Build from Source

To build from source:

1. Clone the repository:
   ```bash
   git clone https://github.com/AndPuQing/gflow.git
   cd gflow
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

   Executables will be in `target/release/`.

3. Install to the system (optional):
   ```bash
   cargo install --path .
   ```

## Verify Installation

After installation, check the commands and version:

```bash
# Check commands
which gflowd ginfo gbatch gqueue gcancel

# Check version
gflowd --version
```

::: tip
If the commands work and `gflowd --version` prints a version, the install is complete.
:::

## Run Checks

### 1. Check tmux
```bash
tmux new-session -d -s test
tmux has-session -t test && echo "tmux ok"
tmux kill-session -t test
```

### 2. Check the daemon and GPU detection (optional)

If you have NVIDIA GPUs, you can verify detection:

```bash
# Optional: create a default config
gflowd init

# Start the daemon
gflowd up

# Check status
gflowd status
```

Check system info and GPUs:
```bash
ginfo
```

If detection works, the output includes GPU information.

## File Locations

gflow uses these directories:

| Location | Purpose |
|----------|---------|
| `~/.config/gflow/gflow.toml` | Configuration file (optional) |
| `~/.local/share/gflow/state.msgpack` | Persistent job state (`state.json` is still read for legacy installs) |
| `~/.local/share/gflow/logs/` | Job output logs |

## Troubleshooting

::: details Issue: Command not found

If you get "command not found" after installation:

1. Check if `~/.cargo/bin` is in your `PATH`:
   ```bash
   echo $PATH | grep -o ~/.cargo/bin
   ```

2. Add it to `~/.bashrc` or `~/.zshrc` if missing:
   ```bash
   export PATH="$HOME/.cargo/bin:$PATH"
   ```

3. Reload the shell:
   ```bash
   source ~/.bashrc  # or ~/.zshrc
   ```
:::

::: details Issue: GPU not detected

1. Check NVIDIA drivers:
   ```bash
   nvidia-smi
   ```

2. Check the NVML library:
   ```bash
   ldconfig -p | grep libnvidia-ml
   ```

3. If detection fails, gflow still works for CPU jobs.
:::

## Updating gflow

::: details If installed via Cargo
```bash
cargo install gflow --force
```
:::

::: details If built from source
```bash
cd gflow
git pull
cargo build --release
cargo install --path . --force
```
:::

## Uninstallation

To remove gflow:

::: warning
The optional data cleanup commands below permanently remove local scheduler state and logs.
:::

```bash
# Stop the daemon first
gflowd down

# Uninstall binaries
cargo uninstall gflow

# Remove configuration and data (optional)
rm -rf ~/.config/gflow
rm -rf ~/.local/share/gflow
```

## Next Steps

After installation, continue with [Quick Start](./quick-start).
