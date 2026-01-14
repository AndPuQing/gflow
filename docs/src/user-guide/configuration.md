# Configuration

This guide covers how to configure gflow for your environment.

## Overview

gflow uses a simple configuration system based on TOML files and environment variables. Most users can use gflow without any configuration, but customization options are available for specific needs.

## Configuration Files

### Default Configuration Location

```
~/.config/gflow/gflow.toml
```

This file is created automatically when you first run gflow commands. If it doesn't exist, gflow uses built-in defaults.

### Configuration File Structure

```toml
[daemon]
# Daemon connection settings
host = "localhost"
port = 59000

# Optional: Specify GPU indices to use (commented out = use all)
# gpus = [0, 1, 2]

# Optional: Log level (error, warn, info, debug, trace)
# log_level = "info"
```

### Custom Configuration Location

Use the `--config` flag (available on all commands, but hidden from help):

```bash
# Use custom config file
gflowd --config /path/to/custom.toml up
gflowd --config /path/to/custom.toml status
ginfo --config /path/to/custom.toml info
gflowd --config /path/to/custom.toml down
gbatch --config /path/to/custom.toml ...
gqueue --config /path/to/custom.toml
```

## Configuration Options

### Daemon Configuration

#### Host and Port

Control where the daemon listens:

```toml
[daemon]
host = "localhost"  # Listen address
port = 59000        # Listen port
```

**Default values**:
- Host: `localhost` (127.0.0.1)
- Port: `59000`

**Use cases**:
- Default is fine for single-machine use
- Change port if 59000 is already in use
- Use `0.0.0.0` to allow remote connections (⚠️ not recommended for security)

#### GPU Selection

Limit which GPUs gflow can use through config files, CLI flags, or runtime commands.

**Config file** (`~/.config/gflow/gflow.toml`):
```toml
[daemon]
# Use only GPUs 0 and 2
gpus = [0, 2]
```

**CLI flag** (overrides config file):
```bash
# Start daemon with GPU restriction
gflowd up --gpus 0,2

# Restart with different GPUs
gflowd restart --gpus 0-3
```

**Runtime control** (change GPUs while daemon is running):
```bash
# Restrict to specific GPUs
gctl set-gpus 0,2

# Use GPU range
gctl set-gpus 0-3

# Allow all GPUs
gctl set-gpus all

# Check current configuration
gctl show-gpus
```

**Supported syntax**:
- Single GPU: `0`
- Comma-separated: `0,2,4`
- Range: `0-3` (expands to 0,1,2,3)
- Mixed: `0-1,3,5-6`

**How it works**:
- Scheduler only allocates jobs to allowed GPUs
- Invalid GPU indices are logged as warnings and ignored
- Running jobs continue unchanged when restrictions change
- Restrictions persist across daemon restarts
- CLI flags override config file settings

**Use cases**:
- Reserve specific GPUs for other applications
- Test with subset of GPUs
- Isolate gflow from other workloads
- Dynamically adjust GPU availability without restarting

**Examples**:

View current GPU configuration:
```bash
$ gctl show-gpus
=== GPU Configuration ===

GPU Restriction: Only GPUs [0, 2] are allowed

=== Detected GPUs ===

GPU 0: Available
GPU 1: In Use (RESTRICTED)
GPU 2: Available
GPU 3: Available (RESTRICTED)
```

Change restriction at runtime:
```bash
# Currently using GPUs 0,2
$ gctl show-gpus
GPU Restriction: Only GPUs [0, 2] are allowed

# Change to use only GPU 0
$ gctl set-gpus 0
GPU restriction updated: only GPUs [0] will be used

# Jobs now can only use GPU 0
# Running jobs on GPU 2 continue, but new jobs won't use it
```

Priority order (highest to lowest):
1. CLI flag: `gflowd up --gpus 0,2`
2. Environment variable: `GFLOW_DAEMON__GPUS='[0,2]'`
3. Config file: `gpus = [0, 2]`
4. Default: All detected GPUs

**Default**: All detected GPUs are available

#### Logging Level

Control daemon verbosity:

```toml
[daemon]
log_level = "info"  # error | warn | info | debug | trace
```

**Levels**:
- `error`: Only critical errors
- `warn`: Warnings and errors
- `info`: General information (default)
- `debug`: Detailed debugging info
- `trace`: Very verbose (includes all internal operations)

## Environment Variables

### Configuration via Environment

gflow supports environment variable configuration with the `GFLOW_` prefix:

```bash
# Set daemon host
export GFLOW_DAEMON_HOST="localhost"

# Set daemon port
export GFLOW_DAEMON_PORT="59000"

# Set log level
export GFLOW_LOG_LEVEL="debug"

# Start daemon with these settings
gflowd up
```

**Precedence**:
1. Command-line arguments (if available)
2. Configuration file (`--config` or default)
3. Environment variables
4. Built-in defaults

## File Locations

### Standard Directories

gflow uses XDG Base Directory specification:

```bash
# Configuration
~/.config/gflow/
  └── gflow.toml          # Main configuration file

# Data (state and logs)
~/.local/share/gflow/
  ├── state.json           # Persistent job state
  └── logs/                # Job output logs
      ├── 1.log
      ├── 2.log
      └── ...

# Runtime (optional, not used by default)
~/.local/share/gflow/
```

## Troubleshooting Configuration

### Issue: Config file not found

**Check location**:
```bash
ls -la ~/.config/gflow/gflow.toml
```

**Solution**: Create default config or specify with `--config`

### Issue: Port already in use

**Check port**:
```bash
lsof -i :59000
```

**Solutions**:
1. Change port in config:
   ```toml
   [daemon]
   port = 59001
   ```

2. Kill process using the port:
   ```bash
   kill <PID>
   ```

## Best Practices

1. **Use default config** unless you have specific needs
2. **Backup state periodically** if job history is important
3. **Clean logs regularly** to manage disk space


## See Also

- [Installation](../getting-started/installation) - Initial setup
- [Quick Start](../getting-started/quick-start) - Basic usage
- [Job Submission](./job-submission) - Submitting jobs
- [GPU Management](./gpu-management) - GPU allocation
