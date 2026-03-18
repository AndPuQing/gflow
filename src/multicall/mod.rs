use std::ffi::OsString;

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
