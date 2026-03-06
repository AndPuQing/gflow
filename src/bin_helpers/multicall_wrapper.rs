use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const MULTICALL_SENTINEL: &str = "__multicall";

pub fn exec(command_name: &str) -> ExitCode {
    match do_exec(command_name) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(127)
        }
    }
}

fn do_exec(command_name: &str) -> Result<ExitCode, String> {
    let forwarded_args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {e}"))?;

    // Developer mode (automatic):
    // If the current working directory is inside the gflow source tree,
    // run via `cargo run --bin gflow` so code changes are picked up without
    // reinstalling binaries. When this wrapper itself is already the repo's
    // Cargo-built artifact, dispatch in-process to avoid spawning a nested
    // `cargo run`.
    if !dev_auto_disabled() {
        if let Some(repo_root) = find_gflow_repo_root() {
            if is_repo_target_binary(&repo_root, &exe) {
                return dispatch_in_process(command_name, forwarded_args);
            }
            return exec_with_cargo(repo_root, multicall_args(command_name, forwarded_args));
        }
    }

    let exe_dir = exe
        .parent()
        .ok_or_else(|| "Failed to determine current executable directory".to_string())?;

    let gflow_path = find_sibling_gflow(exe_dir)
        .unwrap_or_else(|| PathBuf::from(format!("gflow{}", std::env::consts::EXE_SUFFIX)));

    exec_binary(gflow_path, multicall_args(command_name, forwarded_args))
}

fn multicall_args(command_name: &str, rest: Vec<OsString>) -> Vec<OsString> {
    let mut args = Vec::with_capacity(rest.len() + 2);
    args.push(OsString::from(MULTICALL_SENTINEL));
    args.push(OsString::from(command_name));
    args.extend(rest);
    args
}

fn dispatch_in_process(command_name: &str, rest: Vec<OsString>) -> Result<ExitCode, String> {
    let mut argv = Vec::with_capacity(rest.len() + 1);
    argv.push(OsString::from(command_name));
    argv.extend(rest);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to build tokio runtime: {e}"))?;

    runtime
        .block_on(gflow::multicall::dispatch(argv))
        .map_err(|e| format!("{e:?}"))?;

    Ok(ExitCode::SUCCESS)
}

fn exec_binary(gflow_path: PathBuf, args: Vec<OsString>) -> Result<ExitCode, String> {
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
        Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
    }
}

fn exec_with_cargo(repo_root: PathBuf, args: Vec<OsString>) -> Result<ExitCode, String> {
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
        Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
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

fn find_sibling_gflow(exe_dir: &Path) -> Option<PathBuf> {
    let name = format!("gflow{}", std::env::consts::EXE_SUFFIX);
    let p = exe_dir.join(name);
    if p.exists() {
        return Some(p);
    }
    None
}

fn is_repo_target_binary(repo_root: &Path, exe: &Path) -> bool {
    exe.starts_with(repo_root.join("target"))
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
