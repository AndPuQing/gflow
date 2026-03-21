# Installation

This guide will help you install gflow on your system.

::: info Package Name
Install the Python package `runqd`. It provides the `gflowd`, `gbatch`, `gqueue`, `gjob`, `ginfo`, `gcancel`, and `gctl` commands.
:::

## Prerequisites

- **Operating System**: Linux (tested on Ubuntu 20.04+)
- **tmux**: Required for job execution
- **NVIDIA GPU** (optional): For GPU job scheduling
- **NVIDIA drivers** (optional): If using GPU features

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

Install gflow using `uv` (recommended for CLI tools):

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

This will install pre-built binaries for Linux (x86_64, ARM64).

### Install Nightly Build

To try the latest development version, install from TestPyPI:

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

This will compile and install all binaries to `~/.cargo/bin/`, which should be in your `PATH`.

### Method 3: Build from Source

If you want to build from the latest source code:

1. **Clone the repository**:
   ```bash
   git clone https://github.com/AndPuQing/gflow.git
   cd gflow
   ```

2. **Build the project**:
   ```bash
   cargo build --release
   ```

   The executables will be in `target/release/`.

3. **Install to system** (optional):
   ```bash
   cargo install --path .
   ```

## Verify Installation

After installation, verify that gflow is properly installed:

```bash
# Check if commands are available
which gflowd ginfo gbatch gqueue gcancel

# Verify version
gflowd --version
```

::: tip
If the commands are available and `gflowd --version` prints a version, the installation is complete.
:::

## Sanity Check

### 1. tmux
Make sure tmux works:
```bash
tmux new-session -d -s test
tmux has-session -t test && echo "tmux is working!"
tmux kill-session -t test
```

### 2. Daemon + GPU detection (Optional)

If you have NVIDIA GPUs, verify they're detected:

```bash
# (Optional) Create a config file with sensible defaults
gflowd init

# Start the daemon
gflowd up

# Verify it started
gflowd status
```

Check system info and GPU allocation:
```bash
ginfo
```

The daemon shows GPU information if NVIDIA GPUs are available.

## File Locations

gflow uses the following directories:

| Location | Purpose |
|----------|---------|
| `~/.config/gflow/gflow.toml` | Configuration file (optional) |
| `~/.local/share/gflow/state.msgpack` | Persistent job state (`state.json` is still read for legacy installs) |
| `~/.local/share/gflow/logs/` | Job output logs |

## Troubleshooting

::: details Issue: Command not found

If you get "command not found" after installation:

1. **Check if `~/.cargo/bin` is in your PATH**:
   ```bash
   echo $PATH | grep -o ~/.cargo/bin
   ```

2. **Add to PATH** if missing (add to `~/.bashrc` or `~/.zshrc`):
   ```bash
   export PATH="$HOME/.cargo/bin:$PATH"
   ```

3. **Reload shell**:
   ```bash
   source ~/.bashrc  # or ~/.zshrc
   ```
:::

::: details Issue: GPU not detected

1. **Check NVIDIA drivers**:
   ```bash
   nvidia-smi
   ```

2. **Verify NVML library**:
   ```bash
   ldconfig -p | grep libnvidia-ml
   ```

3. If GPU detection fails, gflow will still work but won't manage GPU resources.
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

Now that gflow is installed, head to the [Quick Start Guide](./quick-start) to learn how to use it!
