# Template Generation Scripts

## Overview

The `generate_template.py` script automatically generates the GFLOW job script template from the `AddArgs` struct definition in `src/bin/gbatch/cli.rs`. This ensures that the template always stays in sync with the available command-line arguments.

## How It Works

1. **Parse Phase**: The script parses `src/bin/gbatch/cli.rs` using regex patterns to extract:
   - Field names
   - Help text from `#[arg(help = "...")]` attributes or doc comments
   - Type information

2. **Generate Phase**: Creates a bash script template with:
   - GFLOW directive comments for each field
   - Example values appropriate for each field type
   - Help text as inline comments

3. **Output**: Writes to `src/bin/gbatch/script_template.sh`, which is then included in the binary via `include_str!()` in `new.rs`

## Usage

### Manual Generation

```bash
python3 scripts/generate_template.py
```

### Automatic Generation with Pre-Commit Hook

The template is automatically regenerated whenever `src/bin/gbatch/cli.rs` is modified and committed:

```bash
# Install pre-commit (if not already installed)
pip install pre-commit

# Install the hooks
pre-commit install

# Now the template will regenerate automatically on commit
git add src/bin/gbatch/cli.rs
git commit -m "Add new field to AddArgs"
# ✓ Generated template with 7 directives  <- runs automatically
```

## Example: Adding a New Field

When you add a new field to `AddArgs`:

```rust
// In src/bin/gbatch/cli.rs
pub struct AddArgs {
    // ... existing fields ...

    /// Memory limit in GB
    #[arg(long)]
    pub memory: Option<u32>,
}
```

The pre-commit hook will automatically:
1. Detect the change to `cli.rs`
2. Run `generate_template.py`
3. Update `script_template.sh` with the new directive:
   ```bash
   # GFLOW --memory=1  # Memory limit in GB
   ```
4. Include the updated template in the commit

## Benefits

- **Zero Maintenance**: No manual template updates needed
- **Always in Sync**: Template automatically reflects available options
- **Type Safety**: Compile-time guarantee that template exists
- **Documentation**: Help text from code appears in template

## Files

- `scripts/generate_template.py` - Template generator script
- `src/bin/gbatch/cli.rs` - Source of truth for available options
- `src/bin/gbatch/script_template.sh` - Generated template (auto-generated, commit to repo)
- `src/bin/gbatch/commands/new.rs` - Uses `include_str!()` to embed template
- `.pre-commit-config.yaml` - Hook configuration
