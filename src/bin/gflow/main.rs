use std::ffi::OsString;
use std::process::ExitCode;

mod completion;
mod gbatch;
mod gcancel;
mod gctl;
mod gflowd;
mod ginfo;
mod gjob;
mod gqueue;

const MULTICALL_SENTINEL: &str = "__multicall";

#[tokio::main]
async fn main() -> ExitCode {
    match real_main().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:?}");
            ExitCode::from(1)
        }
    }
}

async fn real_main() -> anyhow::Result<()> {
    let mut it = std::env::args_os();
    let _program = it.next();

    let Some(first) = it.next() else {
        print_top_level_help();
        return Ok(());
    };

    if first == MULTICALL_SENTINEL {
        let Some(cmd) = it.next() else {
            print_top_level_help();
            anyhow::bail!("Missing subcommand for {MULTICALL_SENTINEL}");
        };
        let argv = argv_with_program_name(cmd, it.collect());
        return dispatch(argv).await;
    }

    let argv = argv_with_program_name(first, it.collect());
    dispatch(argv).await
}

fn argv_with_program_name(program: OsString, rest: Vec<OsString>) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(rest.len() + 1);
    argv.push(program);
    argv.extend(rest);
    argv
}

async fn dispatch(argv: Vec<OsString>) -> anyhow::Result<()> {
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
        "gqueue" => gqueue::run(argv).await,
        _ => {
            print_top_level_help();
            anyhow::bail!(
                "Unknown command '{}'. Expected one of: gbatch, gcancel, gctl, gflowd, ginfo, gjob, gqueue",
                program.to_string_lossy()
            );
        }
    }
}

fn print_top_level_help() {
    eprintln!(
        "gflow (multi-call)\n\nUsage:\n  gflow {MULTICALL_SENTINEL} <command> [args...]\n  gflow <command> [args...]\n\nCommands:\n  gbatch\n  gcancel\n  gctl\n  gflowd\n  ginfo\n  gjob\n  gqueue\n"
    );
}
