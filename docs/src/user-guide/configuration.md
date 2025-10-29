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
gflowd --config /path/to/custom.toml
gctl --config /path/to/custom.toml status
gbatch --config /path/to/custom.toml --command "..."
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

Limit which GPUs gflow can use:

```toml
[daemon]
# Use only GPUs 0 and 2
gpus = [0, 2]
```

**Use cases**:
- Reserve specific GPUs for other applications
- Test with subset of GPUs
- Isolate gflow from other workloads

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
gflowd
```

**Precedence**:
1. Command-line arguments (if available)
2. Configuration file (`--config` or default)
3. Environment variables
4. Built-in defaults

### Setting CUDA Devices System-wide

To limit CUDA devices before gflow:

```bash
# Make only GPU 0 visible to gflow
export CUDA_VISIBLE_DEVICES=0
gctl up

# gflow will only see and manage GPU 0
gctl info
```

**Warning**: This affects all CUDA applications, not just gflow.

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

### Customizing Directories

While not officially supported, you can use environment variables:

```bash
# Custom config directory
export XDG_CONFIG_HOME="$HOME/my-config"
# Config will be at: $HOME/my-config/gflow/gflow.toml

# Custom data directory
export XDG_DATA_HOME="$HOME/my-data"
# State will be at: $HOME/my-data/gflow/state.json
# Logs will be at: $HOME/my-data/gflow/logs/
```

## Configuration Management

### View Current Configuration

```bash
# Check daemon status (shows host:port)
gctl status

# View system info
gctl info

# The config file itself
cat ~/.config/gflow/gflow.toml
```

### Reset Configuration

Remove configuration to use defaults:

```bash
# Stop daemon first
gctl down

# Remove config file
rm ~/.config/gflow/gflow.toml

# Restart daemon (uses defaults)
gctl up
```

### Configuration Cleanup

Use the cleanup option (undocumented feature):

```bash
gflowd --cleanup
```

This removes the configuration file and resets to defaults.

## Advanced Configuration

### Multiple gflow Instances

Run multiple independent gflow instances with different configs:

**Instance 1** (default):
```toml
# ~/.config/gflow/gflow.toml
[daemon]
port = 59000
```

```bash
gctl up
```

**Instance 2** (custom):
```toml
# ~/gflow-dev/config.toml
[daemon]
port = 59001
```

```bash
gflowd --config ~/gflow-dev/config.toml &
gctl --config ~/gflow-dev/config.toml status
gbatch --config ~/gflow-dev/config.toml --command "..."
```

**Use cases**:
- Testing new features without affecting production
- Separate job queues for different projects
- Different GPU allocations for different teams

### Per-Project Configuration

Create a project-specific config:

```bash
# Project directory
cd my-ml-project/

# Create local config
cat > gflow.toml << 'EOF'
[daemon]
host = "localhost"
port = 59001
gpus = [0, 1]  # Use only first 2 GPUs for this project
EOF

# Use with --config
gbatch --config ./gflow.toml --gpus 1 --command "python train.py"
```

**Tip**: Add to `.gitignore`:
```bash
echo "gflow.toml" >> .gitignore
```

### GPU Partitioning

Divide GPUs among users or projects:

**User A** (GPUs 0-1):
```toml
# ~/.config/gflow/gflow-userA.toml
[daemon]
port = 59000
gpus = [0, 1]
```

**User B** (GPUs 2-3):
```toml
# ~/.config/gflow/gflow-userB.toml
[daemon]
port = 59001
gpus = [2, 3]
```

Each user runs their own daemon instance.

## Daemon Control

### Starting the Daemon

```bash
# Default config
gctl up

# Custom config
gflowd --config /path/to/config.toml

# With verbosity
gflowd -vv  # debug level
gflowd -vvv  # trace level
```

### Stopping the Daemon

```bash
gctl down
```

### Checking Status

```bash
$ gctl status
gflowd is running (PID: 12345)

# Or if not running:
gflowd is not running
```

### Daemon Persistence

The daemon runs in a tmux session:

```bash
# Attach to daemon session
tmux attach -t gflow_server

# Detach without stopping (Ctrl-B, then D)

# View daemon logs
tmux attach -t gflow_server
# Then scroll up (Ctrl-B, then [)
```

## State Persistence

### Job State

Job state is automatically persisted to disk:

```bash
~/.local/share/gflow/state.json
```

**When state is saved**:
- When jobs are submitted
- When job states change
- Periodically during daemon operation
- When daemon shuts down

**State recovery**:
- Daemon reads state on startup
- Jobs resume from their previous state
- Running jobs are marked as failed (tmux sessions stopped)

### Manual State Management

**Backup state**:
```bash
cp ~/.local/share/gflow/state.json ~/.local/share/gflow/state.json.backup
```

**Clear all job history**:
```bash
# Stop daemon first!
gctl down

# Remove state file
rm ~/.local/share/gflow/state.json

# Restart (fresh state)
gctl up
```

**Restore state**:
```bash
gctl down
cp state.json.backup ~/.local/share/gflow/state.json
gctl up
```

## Logging

### Job Logs

Automatic log capture to files:

```bash
~/.local/share/gflow/logs/<job_id>.log
```

**Features**:
- Automatic directory creation
- Real-time log writing via `tmux pipe-pane`
- Logs persist after job completion
- No size limits (manage manually if needed)

**Managing logs**:
```bash
# View recent logs
ls -lt ~/.local/share/gflow/logs/ | head -10

# Clean old logs
find ~/.local/share/gflow/logs/ -name "*.log" -mtime +30 -delete

# Archive logs
tar -czf logs-$(date +%Y%m%d).tar.gz ~/.local/share/gflow/logs/
```

### Daemon Logs

Daemon logs appear in its tmux session:

```bash
# View daemon logs
tmux attach -t gflow_server

# Capture daemon logs to file
tmux capture-pane -t gflow_server -p > daemon.log
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

### Issue: GPUs not detected

**Check config**:
```toml
[daemon]
# Make sure you don't have invalid GPU indices
# gpus = [0, 1, 2, 3]  # Comment out to use all
```

**Verify GPUs**:
```bash
nvidia-smi
gctl info
```

### Issue: Can't connect to daemon

**Check**:
1. Daemon running: `gctl status`
2. Correct host/port in config
3. Firewall settings (if using custom host)

**Solution**:
```bash
# Restart daemon
gctl down
gctl up

# Check connection
gctl status
```

## Security Considerations

### Local-only Access

By default, gflow only accepts local connections:

```toml
[daemon]
host = "localhost"  # Only local access
```

**Don't expose to network** unless you understand the risks:
```toml
[daemon]
host = "0.0.0.0"  # ⚠️ Accepts connections from any network interface
```

### File Permissions

Protect your configuration and state:

```bash
# Restrict config file
chmod 600 ~/.config/gflow/gflow.toml

# Restrict state file
chmod 600 ~/.local/share/gflow/state.json

# Restrict logs directory
chmod 700 ~/.local/share/gflow/logs/
```

### Multi-user Systems

On shared systems:
- Each user should run their own daemon instance
- Use different ports for each user
- Set proper file permissions on logs and state

```bash
# User 1
# ~/.config/gflow/gflow.toml
[daemon]
port = 59000

# User 2
# ~/.config/gflow/gflow.toml
[daemon]
port = 59001
```

## Best Practices

1. **Use default config** unless you have specific needs
2. **Version control your config** for project-specific settings
3. **Document custom configs** for your team
4. **Backup state periodically** if job history is important
5. **Clean logs regularly** to manage disk space
6. **Use meaningful port numbers** for multiple instances
7. **Test config changes** before deploying to production
8. **Monitor daemon logs** when debugging issues
9. **Set appropriate permissions** on config and state files
10. **Use environment variables** for CI/CD automation

## Configuration Examples

### Example 1: Minimal Config

Use defaults for everything:

```toml
# ~/.config/gflow/gflow.toml
# Empty file - all defaults
```

### Example 2: Custom Port

```toml
[daemon]
port = 59001
```

### Example 3: Limited GPU Access

```toml
[daemon]
# Only use GPUs 0 and 2
gpus = [0, 2]
```

### Example 4: Debug Mode

```toml
[daemon]
log_level = "debug"
```

### Example 5: Multi-instance Setup

**Production** (~/.config/gflow/gflow.toml):
```toml
[daemon]
port = 59000
gpus = [0, 1]
log_level = "info"
```

**Development** (~/gflow-dev/config.toml):
```toml
[daemon]
port = 59001
gpus = [2, 3]
log_level = "debug"
```

## See Also

- [Installation](../getting-started/installation.md) - Initial setup
- [Quick Start](../getting-started/quick-start.md) - Basic usage
- [Job Submission](./job-submission.md) - Submitting jobs
- [GPU Management](./gpu-management.md) - GPU allocation
