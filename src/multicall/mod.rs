use std::ffi::OsString;
use std::path::{Path, PathBuf};

mod completion;

pub mod gbatch;
pub mod gcancel;
pub mod gctl;
pub mod gflowd;
pub mod ginfo;
pub mod gjob;
pub mod gqueue;
pub mod gstats;
pub mod mcp;

/// Resolve the executable that can parse the `__multicall` sentinel.
///
/// The `gflow` binary is the real multicall dispatcher; the sibling wrapper
/// binaries (`gflowd`, `gjob`, …) `execv` into it, so `current_exe()` already
/// points at the dispatcher in release installs. But debug builds run straight
/// from `target/debug` dispatch in-process, leaving `current_exe()` at the
/// wrapper itself, which does not understand `__multicall`. Prefer a sibling
/// `gflow` binary next to the current executable (the same layout the wrapper
/// resolves via `find_sibling_gflow`), falling back to `current_exe()`.
pub fn multicall_executable() -> std::io::Result<PathBuf> {
    Ok(prefer_sibling_gflow(&std::env::current_exe()?))
}

/// Prefer a sibling `gflow` binary next to `exe` (multicall-capable), else the
/// given executable itself.
fn prefer_sibling_gflow(exe: &Path) -> PathBuf {
    let Some(dir) = exe.parent() else {
        return exe.to_path_buf();
    };
    let gflow = dir.join(format!("gflow{}", std::env::consts::EXE_SUFFIX));
    if gflow.is_file() {
        gflow
    } else {
        exe.to_path_buf()
    }
}

pub async fn dispatch(argv: Vec<OsString>) -> anyhow::Result<()> {
    let Some(program) = argv.first() else {
        print_top_level_help();
        return Ok(());
    };

    match program.to_string_lossy().as_ref() {
        "gbatch" => gbatch::run(argv).await,
        "gcancel" => gcancel::run(argv).await,
        "gctl" => gctl::run(argv).await,
        "gflowd" => gflowd::run(argv).await,
        "ginfo" => ginfo::run(argv).await,
        "gjob" => gjob::run(argv).await,
        "mcp" => mcp::run(argv).await,
        "gqueue" => gqueue::run(argv).await,
        "gstats" => gstats::run(argv).await,
        _ => {
            print_top_level_help();
            anyhow::bail!(
                "Unknown command '{}'. Expected one of: gbatch, gcancel, gctl, gflowd, ginfo, gjob, mcp, gqueue, gstats",
                program.to_string_lossy()
            );
        }
    }
}

pub fn print_top_level_help() {
    eprintln!(
        "gflow (multi-call)\n\nUsage:\n  gflow __multicall <command> [args...]\n  gflow <command> [args...]\n\nCommands:\n  gbatch\n  gcancel\n  gctl\n  gflowd\n  ginfo\n  gjob\n  mcp\n  gqueue\n  gstats\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn bin_name(prefix: &str) -> String {
        format!("{prefix}{}", std::env::consts::EXE_SUFFIX)
    }

    #[test]
    fn prefer_sibling_gflow_picks_sibling_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let gflow = tmp.path().join(bin_name("gflow"));
        let gflowd = tmp.path().join(bin_name("gflowd"));
        fs::write(&gflow, "").unwrap();
        fs::write(&gflowd, "").unwrap();

        // A wrapper next to the dispatcher must resolve to the dispatcher so
        // the daemon start command can parse `__multicall`.
        assert_eq!(prefer_sibling_gflow(&gflowd), gflow);
        // The dispatcher itself stays put.
        assert_eq!(prefer_sibling_gflow(&gflow), gflow);
    }

    #[test]
    fn prefer_sibling_gflow_falls_back_to_current_exe() {
        let tmp = tempfile::tempdir().unwrap();
        let gflowd = tmp.path().join(bin_name("gflowd"));
        fs::write(&gflowd, "").unwrap();

        assert_eq!(prefer_sibling_gflow(&gflowd), gflowd);
    }

    #[test]
    fn prefer_sibling_gflow_handles_missing_parent() {
        // A bare filename (no parent) must fall back to itself, not panic.
        assert_eq!(prefer_sibling_gflow(Path::new(bin_name("gflowd").as_str())), PathBuf::from(bin_name("gflowd")));
    }
}
