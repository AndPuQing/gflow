# Template Generation

## Overview

The GFLOW job script template is **automatically generated at runtime** from the `AddArgs` struct using clap's metadata introspection. This ensures the template always stays in sync with available command-line arguments.

## How It Works

When you run `gbatch new <name>`, the template is generated dynamically in `src/bin/gbatch/commands/new.rs`:

1. **Introspect CLI Metadata**: Uses `clap::CommandFactory` to get metadata from `AddArgs`
2. **Filter Directives**: Only includes directives that are actually honored when parsed from script files (see `SCRIPT_SUPPORTED_DIRECTIVES`)
3. **Get Defaults**: Uses `Job::default()` to show accurate default values
4. **Generate Template**: Creates bash script with `## GFLOW` directives (inactive by default)

**Supported Script Directives** (honored when parsed from `# GFLOW` lines in scripts):
- `--gpus` - GPU count to request
- `--priority` - Job priority (0-255)
- `--conda-env` - Conda environment to activate
- `--depends-on` - Job dependency (ID, `@`, or `@~N`)
- `--time` - Time limit (HH:MM:SS, MM:SS, MM, or seconds)
- `--memory` - Memory limit (100G, 1024M, or MB)

**CLI-Only Options** (must be passed via command line, not script):
- `--array` - Job array specification
- `--param` - Parameter sweep
- `--param-file` - Load parameters from CSV
- `--max-concurrent` - Max concurrent jobs
- `--name` - Custom job name
- `--name-template` - Job name template
- `--auto-close` - Auto-close tmux on success
- `--dry-run` - Preview without submitting

## Benefits

- ✅ **Single Source of Truth**: Defaults come from `Job::default()`, help text from clap attributes
- ✅ **Zero Drift**: Template reflects actual code behavior
- ✅ **Automatic Updates**: New CLI args appear automatically (if added to `SCRIPT_SUPPORTED_DIRECTIVES`)
- ✅ **Compile-Time Checks**: Rust compiler catches breaking changes
- ✅ **No External Dependencies**: Pure Rust, no Python needed
- ✅ **Safe by Default**: Uses `## GFLOW` prefix so examples don't activate until user changes to `# GFLOW`

## Adding a New Script Directive

1. Add the field to `AddArgs` in `src/bin/gbatch/cli.rs`:
   ```rust
   /// Your new directive
   #[arg(long)]
   pub your_field: Option<YourType>,
   ```

2. Update script parsing in `src/bin/gbatch/commands/add.rs` to use the field from `script_args`

3. Add the long name to `SCRIPT_SUPPORTED_DIRECTIVES` in `src/bin/gbatch/commands/new.rs`

4. Add example value and default description to the helper functions in `new.rs`

That's it! The template will automatically include it.

## Files

- `src/bin/gbatch/commands/new.rs` - Template generation logic
- `src/bin/gbatch/cli.rs` - Source of truth for available options
- `src/bin/gbatch/commands/add.rs` - Script parsing logic (defines which directives are honored)
- `src/core/job.rs` - Job struct with Default implementation
