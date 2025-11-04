#!/usr/bin/env python3
"""
Generate GFLOW job script template from AddArgs struct definition.

This script parses src/bin/gbatch/cli.rs to extract the AddArgs struct fields
and generates a template with all available GFLOW directives.
"""

import re
import sys
from pathlib import Path
from typing import List, Tuple


def parse_add_args(cli_file: Path) -> List[Tuple[str, str, str]]:
    """
    Parse AddArgs struct from cli.rs and extract field information.

    Returns:
        List of (field_name, help_text, example_value) tuples
    """
    content = cli_file.read_text()

    # Find AddArgs struct
    struct_match = re.search(r"pub struct AddArgs\s*\{(.*?)\n\}", content, re.DOTALL)

    if not struct_match:
        raise ValueError("Could not find AddArgs struct in cli.rs")

    struct_body = struct_match.group(1)

    # Parse fields - handle both with and without #[arg] attributes
    directives = []

    # Pattern for field with attributes
    field_with_attr_pattern = re.compile(
        r"#\[arg\((.*?)\)\]\s*(?:///[^\n]*\n\s*)*pub\s+(\w+):\s*", re.DOTALL
    )

    # Pattern for field without attributes (with doc comments)
    field_simple_pattern = re.compile(
        r"(?:///\s*([^\n]*)\n\s*)+pub\s+(\w+):\s*Option<String>", re.MULTILINE
    )

    # First, collect fields with #[arg] attributes
    for match in field_with_attr_pattern.finditer(struct_body):
        attr_content = match.group(1)
        field_name = match.group(2)

        # Skip positional arguments
        if "trailing_var_arg" in attr_content or field_name == "script_or_command":
            continue

        # Extract help text
        help_match = re.search(r'help\s*=\s*"([^"]*)"', attr_content)
        help_text = help_match.group(1) if help_match else ""

        # Extract long name (defaults to field_name with underscores replaced by hyphens)
        long_match = re.search(r'long\s*=\s*"([^"]*)"', attr_content)
        if long_match:
            long_name = long_match.group(1)
        elif "long" in attr_content:
            long_name = field_name.replace("_", "-")
        else:
            # Field uses short name only, derive long from field name
            long_name = field_name.replace("_", "-")

        # Generate example value based on field name
        example = generate_example(field_name)

        directives.append((long_name, help_text, example))

    # Also collect simple fields with just doc comments
    for match in field_simple_pattern.finditer(struct_body):
        help_text = match.group(1).strip()
        field_name = match.group(2)

        # Skip if already processed
        if any(d[0] == field_name.replace("_", "-") for d in directives):
            continue

        long_name = field_name.replace("_", "-")
        example = generate_example(field_name)

        directives.append((long_name, help_text, example))

    return directives


def generate_example(field_name: str) -> str:
    """Generate example value for a field based on its name."""
    examples = {
        "gpus": "1",
        "priority": "10",
        "conda_env": "your-env-name",
        "depends_on": "123",
        "time": "1:30:00",
        "array": "1-10",
        "name": "your-custom-job-name",
    }
    return examples.get(field_name, "value")


def generate_template(directives: List[Tuple[str, str, str]]) -> str:
    """Generate the script template content."""
    header = """#!/bin/bash
#
# =========================================  gflow  =========================================
#  ██████╗ ███████╗██╗      ██████╗ ██╗    ██╗
# ██╔════╝ ██╔════╝██║     ██╔═══██╗██║    ██║
# ██║  ███╗█████╗  ██║     ██║   ██║██║ █╗ ██║
# ██║   ██║██╔══╝  ██║     ██║   ██║██║███╗██║
# ╚██████╔╝██║     ███████╗╚██████╔╝╚███╔███╔╝
#  ╚═════╝ ╚═╝     ╚══════╝ ╚═════╝  ╚══╝╚══╝
#
# A lightweight, single-node GPU job scheduler
# ==========================================================================================
#
# Job Configuration
# -----------------
# Use the GFLOW directives below to configure your job.
# These settings can be overridden by command-line arguments.
#
"""

    directive_lines = []
    for long_name, help_text, example in directives:
        if help_text:
            directive_lines.append(f"# GFLOW --{long_name}={example}  # {help_text}")
        else:
            directive_lines.append(f"# GFLOW --{long_name}={example}")

    footer = """
# --- Your script starts here ---
echo "Starting gflow job..."
echo "Running on node: $HOSTNAME"
sleep 20
echo "Job finished successfully."
"""

    return header + "\n" + "\n".join(directive_lines) + footer


def main():
    # Paths
    repo_root = Path(__file__).parent.parent
    cli_file = repo_root / "src" / "bin" / "gbatch" / "cli.rs"
    output_file = repo_root / "src" / "bin" / "gbatch" / "script_template.sh"

    if not cli_file.exists():
        print(f"Error: Could not find {cli_file}", file=sys.stderr)
        sys.exit(1)

    # Parse and generate
    try:
        directives = parse_add_args(cli_file)
        template = generate_template(directives)

        # Write output
        output_file.write_text(template)
        print(f"✓ Generated template with {len(directives)} directives")
        print(f"  Output: {output_file}")

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
