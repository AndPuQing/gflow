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
    let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {e}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "Failed to determine current executable directory".to_string())?;

    let gflow_path = find_sibling_gflow(exe_dir)
        .unwrap_or_else(|| PathBuf::from(format!("gflow{}", std::env::consts::EXE_SUFFIX)));

    let mut args: Vec<OsString> = Vec::new();
    args.push(OsString::from(MULTICALL_SENTINEL));
    args.push(OsString::from(command_name));
    args.extend(std::env::args_os().skip(1));

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

fn find_sibling_gflow(exe_dir: &std::path::Path) -> Option<PathBuf> {
    let name = format!("gflow{}", std::env::consts::EXE_SUFFIX);
    let p = exe_dir.join(name);
    if p.exists() {
        return Some(p);
    }
    None
}
