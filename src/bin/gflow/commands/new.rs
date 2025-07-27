use crate::cli;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const SCRIPT_TEMPLATE: &str = r#"#!/bin/bash
#
# ==================================================
# GFLOW Job Configuration
#
# Use the GFLOW directives below to configure your job.
# These settings can be overridden by command-line arguments.
# ==================================================

# GFLOW --gpus=1
# GFLOW --priority=10
# GFLOW --conda-env=your-env-name
# GFLOW --gpu-mem=4096
# GFLOW --depends-on=123

# --- Your script starts here ---
echo "Starting gflow job..."
echo "Running on node: $HOSTNAME"
sleep 20
echo "Job finished successfully."
"#;

pub(crate) fn handle_new(new_args: cli::NewArgs) -> Result<()> {
    let job_name = &new_args.name;
    let job_dir = Path::new(job_name);

    if job_dir.exists() {
        anyhow::bail!("Directory '{}' already exists.", job_name);
    }

    fs::create_dir(job_dir).with_context(|| format!("Failed to create directory '{job_name}'"))?;

    let script_path = job_dir.join("run.sh");
    fs::write(&script_path, SCRIPT_TEMPLATE)
        .with_context(|| format!("Failed to write to script file '{script_path:?}'"))?;

    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
    }

    log::info!(
        "Successfully created new job '{}' in directory '{}'",
        job_name,
        job_name
    );
    Ok(())
}
