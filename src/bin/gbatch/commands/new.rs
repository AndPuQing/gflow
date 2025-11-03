use crate::cli;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

// Template is generated from AddArgs struct by scripts/generate_template.py
const SCRIPT_TEMPLATE: &str = include_str!("../script_template.sh");

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

    log::info!("Successfully created new job '{job_name}' in directory '{job_dir:?}'.");
    Ok(())
}
