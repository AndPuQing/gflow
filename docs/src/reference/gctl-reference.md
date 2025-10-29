# gctl Command Reference

Complete reference for the `gctl` command - gflow's daemon control tool.

## Synopsis

```bash
gctl <COMMAND> [OPTIONS]
```

## Description

`gctl` controls the gflow daemon (`gflowd`) and displays system information. It provides commands to start, stop, and check the status of the scheduler, as well as view GPU allocation and system details.

## Commands

### `up`

Start the gflow scheduler daemon.

**Syntax**:
```bash
gctl up
```

**Behavior**:
- Starts `gflowd` in a tmux session named `gflow_server`
- Loads previous job state from `~/.local/share/gflow/state.json`
- Detects available GPUs
- Begins scheduling loop (checks every 5 seconds)
- Returns immediately; daemon runs in background

**Output**:
```bash
gflowd started.
```

**Examples**:
```bash
# Start daemon
gctl up

# Start with custom config
gctl --config ~/my-config.toml up
```

**Notes**:
- If daemon is already running, shows error
- Creates necessary directories automatically
- Initializes HTTP API server on configured port

**Equivalent commands**:
```bash
gctl start  # Alias for up (if available)
```

### `down`

Stop the gflow scheduler daemon.

**Syntax**:
```bash
gctl down
```

**Behavior**:
- Saves current job state to disk
- Stops the scheduler loop
- Shuts down HTTP API server
- Kills the `gflow_server` tmux session
- Marks running jobs as failed (they're terminated)

**Output**:
```bash
gflowd stopped.
```

**Examples**:
```bash
# Stop daemon
gctl down

# Stop with custom config
gctl --config ~/my-config.toml down
```

**Notes**:
- If daemon is not running, shows error or returns silently
- Does not cancel queued jobs; they remain in queue
- Running jobs are terminated and marked as failed
- Job state is preserved for restart

**Equivalent commands**:
```bash
gctl stop  # Alias for down (if available)
```

### `status`

Show daemon status.

**Syntax**:
```bash
gctl status
```

**Output examples**:

**Daemon running**:
```bash
gflowd is running (PID: 12345)
```

**Daemon not running**:
```bash
gflowd is not running
```

**Examples**:
```bash
# Check status
gctl status

# Use in scripts
if gctl status | grep -q "running"; then
    echo "Daemon is active"
fi
```

**Notes**:
- Checks for `gflow_server` tmux session
- Verifies process is actually running (not just zombie session)
- Returns quickly

### `info`

Display system information and GPU allocation.

**Syntax**:
```bash
gctl info
```

**Output example**:
```bash
<!-- cmdrun gctl info -->
```

**Notes**:
- Requires daemon to be running
- Shows real-time GPU allocation
- Useful for capacity planning
- GPU detection requires NVML library

## Global Options

### `--config <PATH>`

Use custom configuration file (hidden option).

**Example**:
```bash
gctl --config /path/to/custom.toml up
gctl --config /path/to/custom.toml status
```

**Use cases**:
- Multiple gflow instances
- Testing configurations
- Project-specific settings

### `--help`, `-h`

Display help message.

```bash
$ gctl --help
<!-- cmdrun gctl --help -->

$ gctl up --help
<!-- cmdrun gctl up --help -->
```

### `--version`, `-V`

Display version information.

```bash
$ gctl --version
<!-- cmdrun gctl --version -->
```

## Examples

### Basic Workflow

```bash
# Start scheduler
gctl up

# Check status
gctl status

# View system info
gctl info

# Stop scheduler
gctl down
```

### System Monitoring

```bash
# Check daemon status periodically
watch -n 5 gctl status

# Monitor GPU allocation
watch -n 2 gctl info

# Check if daemon is running (script)
if gctl status | grep -q "running"; then
    echo "✓ Daemon active"
else
    echo "✗ Daemon inactive"
    gctl up
fi
```

### Multi-instance Setup

```bash
# Instance 1 (default config)
gctl up

# Instance 2 (custom config)
gctl --config ~/dev-config.toml up

# Check both
gctl status
gctl --config ~/dev-config.toml status
```

### Troubleshooting

```bash
# Daemon won't start?
gctl down  # Ensure clean state
tmux kill-session -t gflow_server  # Force cleanup
gctl up

# Check connection
gctl status
gctl info

# Restart daemon
gctl down && gctl up
```

## Integration Examples

### Shell Script: Start if Not Running

```bash
#!/bin/bash
# ensure_daemon.sh - Ensure daemon is running

if ! gctl status | grep -q "running"; then
    echo "Starting gflow daemon..."
    gctl up
    sleep 2
fi

if gctl status | grep -q "running"; then
    echo "✓ Daemon is running"
else
    echo "✗ Failed to start daemon"
    exit 1
fi
```

### Shell Script: System Dashboard

```bash
#!/bin/bash
# gflow_dashboard.sh - System overview

clear
echo "╔════════════════════════════════════════╗"
echo "║       gflow System Dashboard           ║"
echo "╚════════════════════════════════════════╝"

echo -e "\n=== Daemon Status ==="
gctl status

echo -e "\n=== System Information ==="
gctl info

echo -e "\n=== Running Jobs ==="
gqueue -s Running -f JOBID,NAME,NODES,NODELIST

echo -e "\n=== Queued Jobs ==="
gqueue -s Queued -f JOBID,NAME,NODES,NODELIST
```

### Python Script: Check Daemon

```python
#!/usr/bin/env python3
# check_daemon.py

import subprocess
import sys

def is_daemon_running():
    """Check if gflow daemon is running."""
    result = subprocess.run(
        ['gctl', 'status'],
        capture_output=True,
        text=True
    )
    return 'running' in result.stdout.lower()

def start_daemon():
    """Start gflow daemon."""
    subprocess.run(['gctl', 'up'])

def main():
    if not is_daemon_running():
        print("Daemon not running, starting...")
        start_daemon()
    else:
        print("Daemon is already running")

if __name__ == '__main__':
    main()
```

### Systemd Service (Advanced)

Create a systemd service for auto-start:

```ini
# /etc/systemd/system/gflow.service
[Unit]
Description=gflow job scheduler daemon
After=network.target

[Service]
Type=forking
User=youruser
ExecStart=/home/youruser/.cargo/bin/gctl up
ExecStop=/home/youruser/.cargo/bin/gctl down
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable gflow
sudo systemctl start gflow
sudo systemctl status gflow
```

**Note**: This is an advanced configuration and may require adjustments.

## Daemon Lifecycle

### Startup Sequence

1. User runs `gctl up`
2. Check if daemon already running (via tmux session)
3. Create tmux session `gflow_server`
4. Start `gflowd` process
5. Load configuration from `~/.config/gflow/gflow.toml`
6. Load job state from `~/.local/share/gflow/state.json`
7. Detect GPUs via NVML
8. Start HTTP API server
9. Begin scheduler loop (5-second intervals)
10. Return control to user

### Shutdown Sequence

1. User runs `gctl down`
2. Send shutdown signal to daemon
3. Daemon stops accepting new jobs
4. Save current job state to disk
5. Terminate running jobs (mark as failed)
6. Stop HTTP API server
7. Exit `gflowd` process
8. Kill `gflow_server` tmux session
9. Return control to user

### State Persistence

**What is saved**:
- Job IDs and metadata
- Job states (Queued, Running, etc.)
- Job start/finish times
- GPU allocations
- Job dependencies
- Job priorities
- Time limits

**What is NOT saved**:
- Running job progress (jobs restart from beginning)
- Tmux session state
- Live job output (only what's in logs)

## Troubleshooting

### Issue: Daemon won't start

**Check**:
```bash
# Is tmux installed?
which tmux

# Is port available?
lsof -i :59000

# Any zombie sessions?
tmux ls
```

**Solutions**:
```bash
# Install tmux
sudo apt-get install tmux

# Kill zombie session
tmux kill-session -t gflow_server

# Use different port
# Edit ~/.config/gflow/gflow.toml
[daemon]
port = 59001
```

### Issue: Daemon stops unexpectedly

**Check**:
```bash
# View daemon logs
tmux attach -t gflow_server

# Check system logs
journalctl -u gflow  # if using systemd
```

**Possible causes**:
- Out of memory
- Disk space full
- Configuration error
- System reboot

### Issue: Cannot connect to daemon

**Check**:
```bash
# Is daemon running?
gctl status

# Correct config?
cat ~/.config/gflow/gflow.toml

# Network issues?
curl http://localhost:59000/health  # if API exposed
```

**Solutions**:
```bash
# Restart daemon
gctl down
gctl up

# Check config
gctl --config ~/.config/gflow/gflow.toml status
```

### Issue: GPUs not detected

**Check**:
```bash
# NVIDIA driver
nvidia-smi

# NVML library
ldconfig -p | grep libnvidia-ml
```

**Solutions**:
```bash
# Install NVIDIA drivers
# (distribution-specific)

# Restart daemon
gctl down
gctl up

# Check GPU detection
gctl info
```

## Best Practices

1. **Start daemon on login** (add to `.bashrc` or use systemd)
2. **Check status before submitting** large job batches
3. **Monitor GPU allocation** with `gctl info`
4. **Restart daemon periodically** for long-running systems
5. **Use `down` before system maintenance**
6. **Keep daemon config versioned** for reproducibility
7. **Monitor daemon logs** in tmux for errors
8. **Set up alerting** for daemon failures (if critical)

## See Also

- [gbatch](./gbatch-reference.md) - Job submission reference
- [gqueue](./gqueue-reference.md) - Job queue reference
- [gcancel](./gcancel-reference.md) - Job cancellation reference
- [Configuration](../user-guide/configuration.md) - Configuration guide
- [Installation](../getting-started/installation.md) - Setup guide
- [Quick Reference](./quick-reference.md) - Command cheat sheet
