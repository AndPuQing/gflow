use crate::cli;
use anyhow::{Context, Result};
use clap::CommandFactory;
use gflow::core::job::Job;
use std::fs;
use std::path::Path;

// Directives that are actually honored when parsed from script files
// See: parse_script_content_for_args() and handle_add() usage in add.rs
const SCRIPT_SUPPORTED_DIRECTIVES: &[&str] = &[
    "gpus",
    "priority",
    "conda-env",
    "depends-on",
    "time",
    "memory",
];

/// Generate example value for a field based on its long name
fn generate_example_value(long_name: &str) -> &'static str {
    match long_name {
        "gpus" => "1",
        "priority" => "50",
        "conda-env" => "myenv",
        "depends-on" => "123",
        "time" => "1:30:00",
        "memory" => "4G",
        _ => "value",
    }
}

/// Get the actual default value from Job struct for documentation
fn get_default_description(long_name: &str, defaults: &Job) -> String {
    match long_name {
        "gpus" => format!("{}", defaults.gpus),
        "priority" => format!("{}", defaults.priority),
        "conda-env" => "none".to_string(),
        "time" => "unlimited".to_string(),
        "memory" => "unlimited".to_string(),
        "depends-on" => "none".to_string(),
        _ => "unspecified".to_string(),
    }
}

/// Generate the job script template dynamically from clap metadata
fn generate_script_template() -> String {
    let cmd = cli::GBatch::command();
    let defaults = Job::default();

    // Get all arguments from AddArgs
    let mut directives = Vec::new();

    for arg in cmd.get_arguments() {
        let Some(long) = arg.get_long() else {
            continue;
        };

        // Only include directives that are actually honored from script parsing
        if !SCRIPT_SUPPORTED_DIRECTIVES.contains(&long) {
            continue;
        }

        let example = generate_example_value(long);
        let default = get_default_description(long, &defaults);

        // Get help text and sanitize it (replace newlines to avoid breaking bash comments)
        let help = arg
            .get_help()
            .map(|h| h.to_string().replace('\n', " "))
            .unwrap_or_default();

        let directive = if arg.get_action().takes_values() {
            format!("## GFLOW --{}={}", long, example)
        } else {
            format!("## GFLOW --{}", long)
        };

        let comment = if !help.is_empty() {
            format!("# {} (default: {})", help, default)
        } else {
            format!("# (default: {})", default)
        };

        directives.push(format!("{}\n{}\n", directive, comment));
    }

    let header = "#!/bin/bash
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
# Use GFLOW directives to configure your job. These settings can be overridden by
# command-line arguments when submitting the job.
#
# To activate a directive: change ## to # on the GFLOW line.
# Example: ## GFLOW --gpus=1  becomes  # GFLOW --gpus=1
#
# NOTE: Only the directives shown below are honored when parsed from script files.
# Other options (--array, --param, --name, etc.) must be passed via command line.
#
";

    let footer = "
# --- Your script starts here ---
echo \"Starting gflow job...\"
echo \"Running on node: $HOSTNAME\"

# Add your commands here
# Example:
# python train.py --epochs 100

echo \"Job finished successfully.\"
";

    format!("{}\n{}\n{}", header, directives.join("\n"), footer)
}

pub(crate) fn handle_new(new_args: cli::NewArgs) -> Result<()> {
    let job_name = &new_args.name;

    // Add .sh extension if not present
    let script_path = if job_name.ends_with(".sh") {
        Path::new(job_name).to_path_buf()
    } else {
        Path::new(&format!("{}.sh", job_name)).to_path_buf()
    };

    if script_path.exists() {
        anyhow::bail!("File '{}' already exists.", script_path.display());
    }

    let template = generate_script_template();
    fs::write(&script_path, template)
        .with_context(|| format!("Failed to write to script file '{}'", script_path.display()))?;

    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
    }

    tracing::info!("Created template: {}", script_path.display());
    Ok(())
}
