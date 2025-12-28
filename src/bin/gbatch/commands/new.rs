use crate::cli;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

// Template is generated from AddArgs struct by scripts/generate_template.py
const SCRIPT_TEMPLATE: &str = include_str!("../script_template.sh");

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

    fs::write(&script_path, SCRIPT_TEMPLATE)
        .with_context(|| format!("Failed to write to script file '{}'", script_path.display()))?;

    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
    }

    log::info!("Created template: {}", script_path.display());
    Ok(())
}
