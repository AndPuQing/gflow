# Installation

This guide will help you install gflow on your system.

## Prerequisites

- **Operating System**: Linux (tested on Ubuntu 20.04+)
- **Rust**: Version 1.70+ (for building from source)
- **tmux**: Required for job execution
- **NVIDIA GPU** (optional): For GPU job scheduling
- **NVIDIA drivers** (optional): If using GPU features

### Installing Prerequisites

#### Ubuntu/Debian
```bash
# Install tmux
sudo apt-get update
sudo apt-get install tmux

# Install Rust (if building from source)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### Fedora/RHEL
```bash
# Install tmux
sudo dnf install tmux

# Install Rust (if building from source)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Installation Methods

### Method 1: Quick Install with Pre-built Binaries (Recommended)

This is the fastest way to install gflow on Linux x86_64:

**Using our global CDN** (faster, works in GitHub-restricted regions):
```bash
curl -fsSL https://gflow-releases.puqing.work/install.sh | sh
```

**Or using GitHub**:
```bash
curl -fsSL https://raw.githubusercontent.com/AndPuQing/gflow/main/install.sh | sh
```

The installer automatically:
- Tries the CDN first, falls back to GitHub if needed
- Downloads the latest release binaries
- Installs to `~/.cargo/bin/` (customizable with `GFLOW_INSTALL_DIR`)
- Installs all binaries: `gflowd`, `ginfo`, `gbatch`, `gqueue`, `gcancel`, `gjob`, `gctl`

**Custom installation directory**:
```bash
curl -fsSL https://gflow-releases.puqing.work/install.sh | GFLOW_INSTALL_DIR=/usr/local/bin sh
```

### Method 2: Install via Cargo

Build and install from crates.io:

```bash
cargo install gflow
```

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

### Method 4: Manual Download from CDN or GitHub

Download pre-built binaries directly:

**From CDN**:
- Latest release: `https://gflow-releases.puqing.work/releases/latest/`
- Specific version: `https://gflow-releases.puqing.work/releases/v0.4.5/` (replace with desired version)

**From GitHub**:
- Visit the [Releases page](https://github.com/AndPuQing/gflow/releases)
- Download the `gflow_vX.X.X_x86_64-unknown-linux-musl.tar.gz` archive
- Extract and copy binaries to a directory in your `PATH`

## Verify Installation

After installation, verify that gflow is properly installed:

Check versions:
```bash
$ gflowd --version
<!-- cmdrun gflowd --version -->
```

```bash
$ ginfo --version
<!-- cmdrun ginfo --version -->
```

```bash
$ gbatch --version
<!-- cmdrun gbatch --version -->
```

```bash
$ gqueue --version
<!-- cmdrun gqueue --version -->
```

```bash
$ gcancel --version
<!-- cmdrun gcancel --version -->
```

Verify commands are in PATH:
```bash
$ which ginfo
```

All commands are properly installed and available in your PATH.

## Post-Installation Setup

### 1. Test tmux
Make sure tmux is working:
```bash
tmux new-session -d -s test
tmux has-session -t test && echo "tmux is working!"
tmux kill-session -t test
```

### 2. GPU Detection (Optional)

If you have NVIDIA GPUs, verify they're detected:

```bash
# Start the daemon
$ gflowd up

# Verify it started
$ gflowd status
```

Check system info and GPU allocation:
```bash
$ ginfo
```

The daemon shows GPU information if NVIDIA GPUs are available.

### 3. Create Configuration Directory

gflow will create this automatically, but you can do it manually:

```bash
mkdir -p ~/.config/gflow
mkdir -p ~/.local/share/gflow/logs
```

## Configuration Files

gflow uses the following directories:

| Location | Purpose |
|----------|---------|
| `~/.config/gflow/gflowd.toml` | Configuration file (optional) |
| `~/.local/share/gflow/state.json` | Persistent job state |
| `~/.local/share/gflow/logs/` | Job output logs |

## Troubleshooting

### Issue: Command not found

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

### Issue: GPU not detected

1. **Check NVIDIA drivers**:
   ```bash
   nvidia-smi
   ```

2. **Verify NVML library**:
   ```bash
   ldconfig -p | grep libnvidia-ml
   ```

3. If GPU detection fails, gflow will still work but won't manage GPU resources.

## Updating gflow

### If installed via cargo:
```bash
cargo install gflow --force
```

### If built from source:
```bash
cd gflow
git pull
cargo build --release
cargo install --path . --force
```

## Uninstallation

To remove gflow:

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

---

**Previous**: [Introduction](/) | **Next**: [Quick Start](./quick-start)
