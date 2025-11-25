# gflow Shell Completions

This directory contains enhanced shell completions for all gflow commands with **dynamic job ID completion**.

## Features

### Static Completions (Built-in)
All gflow binaries support generating basic static completions via the `completion` subcommand:
- Subcommands and flags
- Value hints for file paths and commands

### Dynamic Completions (This Directory)
Enhanced completions that provide:
- **Live job ID completion** - suggests actual running/queued/held jobs
- **State-aware completion** - `gjob attach` suggests only Running jobs, `gjob hold` suggests only Queued jobs
- **Conda environment completion** - auto-completes available conda environments
- **Dependency completion** - suggests job IDs and special syntax (`@`, `@~1`, etc.)

## Installation

### Bash

1. Generate and install basic completions:
   ```bash
   mkdir -p ~/.local/share/bash-completion/completions
   gbatch completion bash > ~/.local/share/bash-completion/completions/gbatch
   gjob completion bash > ~/.local/share/bash-completion/completions/gjob
   gqueue completion bash > ~/.local/share/bash-completion/completions/gqueue
   gcancel completion bash > ~/.local/share/bash-completion/completions/gcancel
   ginfo completion bash > ~/.local/share/bash-completion/completions/ginfo
   gflowd completion bash > ~/.local/share/bash-completion/completions/gflowd
   ```

2. Add dynamic completions to your `~/.bashrc`:
   ```bash
   # Enhanced gflow completions with job ID completion
   source /path/to/gflow/completions/gflow-dynamic.bash
   ```

3. Reload your shell:
   ```bash
   source ~/.bashrc
   ```

### Zsh

1. Create completion directory if needed:
   ```zsh
   mkdir -p ~/.zsh/completions
   ```

2. Add to your `~/.zshrc` (before `compinit`):
   ```zsh
   # Add completion directory to fpath
   fpath=(~/.zsh/completions $fpath)

   # Load enhanced gflow completions
   source /path/to/gflow/completions/gflow-dynamic.zsh

   # Initialize completion system
   autoload -Uz compinit && compinit
   ```

3. Reload your shell:
   ```zsh
   source ~/.zshrc
   ```

### Fish

1. Copy the dynamic completion file:
   ```fish
   cp /path/to/gflow/completions/gflow-dynamic.fish ~/.config/fish/completions/
   ```

2. Completions are automatically loaded. Start a new fish session or run:
   ```fish
   source ~/.config/fish/completions/gflow-dynamic.fish
   ```

## Usage Examples

### gjob - Smart Job ID Completion

```bash
# Attach to a running job
gjob attach -j <TAB>
# → Shows only Running jobs: 123, 124, 125

# Hold a queued job
gjob hold -j <TAB>
# → Shows only Queued jobs: 130, 131

# Release a held job
gjob release -j <TAB>
# → Shows only Held jobs: 140

# Show any job
gjob show -j <TAB>
# → Shows all jobs

# Redo with special syntax
gjob redo <TAB>
# → Shows: @ @~1 @~2 @~3 123 124 125 ...
```

### gbatch - Command and File Completion

```bash
# Complete command names
gbatch pyt<TAB>
# → python python3 python3.11 ...

# Complete file paths
gbatch python scripts/tra<TAB>
# → scripts/train.py scripts/train_v2.py

# Complete conda environments
gbatch -c <TAB>
# → base pytorch tensorflow myenv

# Complete job dependencies
gbatch --depends-on <TAB>
# → @ @~1 @~2 123 124 125 ...
```

### gcancel - Job ID Completion

```bash
# Cancel a job
gcancel <TAB>
# → Shows all job IDs: 123 124 125 130 ...

# Supports multiple IDs
gcancel 123 <TAB>
# → Shows remaining job IDs
```

### gqueue - State and Format Completion

```bash
# Filter by states
gqueue --states <TAB>
# → Queued Running Held Completed Failed Cancelled

# Format fields
gqueue --format <TAB>
# → id state time name gpus priority command depends_on
```

## How It Works

### Dynamic Job ID Lookup
The completion scripts call `gqueue` with appropriate filters to get live job data:

```bash
# Get running jobs
gqueue --states Running --format id | tail -n +2 | awk '{print $1}'

# Get all jobs
gqueue --format id | tail -n +2 | awk '{print $1}'
```

### Performance
- Job ID lookups are fast (typically <100ms)
- Results are cached by the shell during tab completion session
- Only queries when actually needed (on <TAB>)

### Config File Support
The completion scripts respect the `--config` flag if present on the command line:
```bash
gjob --config /custom/path/config.toml attach -j <TAB>
# → Uses the custom config to query jobs
```

## Troubleshooting

### Completion not working
1. Ensure gflow binaries are in your PATH:
   ```bash
   which gqueue gbatch gjob
   ```

2. Test manual job lookup:
   ```bash
   gqueue --format id
   ```

3. Reload completion system:
   ```bash
   # Bash
   source ~/.bashrc

   # Zsh
   rm ~/.zcompdump && source ~/.zshrc

   # Fish
   fish_update_completions
   ```

### Slow completion
If completion feels slow, it's likely due to:
- Large number of jobs (>1000)
- Slow disk I/O
- Network filesystem for logs

You can disable dynamic completion by removing/commenting out the dynamic completion source line from your shell config.

### Job IDs not showing
1. Ensure daemon is running:
   ```bash
   gflowd status
   ```

2. Check if jobs exist:
   ```bash
   gqueue --all
   ```

3. Verify gqueue output format:
   ```bash
   gqueue --format id
   ```

## Customization

You can modify the completion scripts to:
- Change which states are suggested for each command
- Add custom completion logic
- Integrate with other tools

The scripts are heavily commented to make customization easy.

## Contributing

If you improve the completions, please submit a PR! Areas for enhancement:
- Caching job IDs for better performance
- Completing job names in addition to IDs
- Smart sorting (e.g., most recent jobs first)
- Integration with other schedulers
