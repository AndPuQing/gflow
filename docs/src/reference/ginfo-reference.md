# ginfo Command Reference

Complete reference for the `ginfo` command — gflow's scheduler inspection tool.

## Synopsis

```bash
ginfo <COMMAND> [OPTIONS]
```

## Description

`ginfo` connects to a running gflow daemon (`gflowd`) and prints scheduler metadata, GPU availability, and which jobs are currently bound to each device. The command is read-only and can be run as often as needed to monitor the system.

If the daemon is unreachable, `ginfo` reports the connection failure without modifying any state.

## Commands

### `info`

Display the current scheduler status and GPU allocation.

**Syntax**
```bash
ginfo
```

**What it shows**
- GPU indices, short UUIDs, and availability
- Jobs currently occupying each GPU (job ID and run name)
- Scheduler metadata (total GPUs detected, availability)

**Examples**
```bash
# Query the default daemon using the default config path
ginfo

# Use a custom configuration file
ginfo --config ~/gflow-dev/config.toml info

# Refresh the view every 2 seconds
watch -n 2 ginfo
```

When the daemon is offline, the command prints a helpful error such as:
```
ginfo: daemon not reachable: ...
```

## Global Options

### `--config <PATH>`

Specify an alternate configuration file when connecting to the daemon.

```bash
ginfo --config /path/to/custom.toml info
```

Use this when running multiple gflow instances or testing non-default settings.

### `-v`, `-vv`, `-q`

Adjust log verbosity for troubleshooting:
- `-v` enables info-level logging
- `-vv` enables debug logging
- `-q` suppresses non-error output

## Usage Patterns

```bash
# Combine with gqueue for a full snapshot
ginfo && gqueue -s Running -f JOBID,NAME,NODES,NODELIST

# Create a lightweight dashboard
watch -n 5 '
  clear
  date
  echo
  ginfo
'
```

`ginfo` is safe to run from scripts, cron jobs, and monitoring tooling because it never mutates scheduler state.
