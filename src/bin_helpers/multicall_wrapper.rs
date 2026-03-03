use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const MULTICALL_SENTINEL: &str = "__multicall";

pub fn exec(command_name: &str) -> ExitCode {
    if let Err(err) = do_exec(command_name) {
        eprintln!("{err}");
        return ExitCode::from(127);
    }
    ExitCode::SUCCESS
}

fn do_exec(command_name: &str) -> Result<(), String> {
    let mut args: Vec<OsString> = Vec::new();
    args.push(OsString::from(MULTICALL_SENTINEL));
    args.push(OsString::from(command_name));
    args.extend(std::env::args_os().skip(1));

    // Developer mode (automatic):
    // If the current working directory is inside the gflow source tree,
    // run via `cargo run --bin gflow` so code changes are picked up without
    // reinstalling binaries.
    if !dev_auto_disabled() {
        if let Some(repo_root) = find_gflow_repo_root() {
            return exec_with_cargo(repo_root, args);
        }
    }

    let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {e}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "Failed to determine current executable directory".to_string())?;

    let gflow_path = find_sibling_gflow(exe_dir)
        .unwrap_or_else(|| PathBuf::from(format!("gflow{}", std::env::consts::EXE_SUFFIX)));

    exec_binary(gflow_path, args)
}

fn exec_binary(gflow_path: PathBuf, args: Vec<OsString>) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(&gflow_path).args(args).exec();
        Err(format!("Failed to exec `{}`: {err}", gflow_path.display()))
    }

    #[cfg(not(unix))]
    {
        let status = Command::new(&gflow_path)
            .args(args)
            .status()
            .map_err(|e| format!("Failed to run `{}`: {e}", gflow_path.display()))?;
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }
}

fn dev_auto_disabled() -> bool {
    std::env::var_os("GFLOW_DISABLE_DEV_AUTO")
        .map(|v| {
            let s = v.to_string_lossy().to_ascii_lowercase();
            s == "1" || s == "true" || s == "yes"
        })
        .unwrap_or(false)
}

fn find_sibling_gflow(exe_dir: &std::path::Path) -> Option<PathBuf> {
    let name = format!("gflow{}", std::env::consts::EXE_SUFFIX);
    let p = exe_dir.join(name);
    if p.exists() {
        return Some(p);
    }
    None
}

fn exec_with_cargo(repo_root: PathBuf, args: Vec<OsString>) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new("cargo")
            .current_dir(&repo_root)
            .arg("run")
            .arg("--bin")
            .arg("gflow")
            .arg("--")
            .args(args)
            .exec();
        Err(format!(
            "Failed to run `cargo run --bin gflow` in `{}`: {err}",
            repo_root.display()
        ))
    }

    #[cfg(not(unix))]
    {
        let status = Command::new("cargo")
            .current_dir(&repo_root)
            .arg("run")
            .arg("--bin")
            .arg("gflow")
            .arg("--")
            .args(args)
            .status()
            .map_err(|e| {
                format!(
                    "Failed to run `cargo run --bin gflow` in `{}`: {e}",
                    repo_root.display()
                )
            })?;
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }
}

fn find_gflow_repo_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        let gflow_entry = dir.join("src/bin/gflow/main.rs");
        if cargo_toml.is_file() && gflow_entry.is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}
