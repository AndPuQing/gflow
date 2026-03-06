use std::ffi::OsString;
use std::process::ExitCode;

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
        gflow::multicall::print_top_level_help();
        return Ok(());
    };

    if first == MULTICALL_SENTINEL {
        let Some(cmd) = it.next() else {
            gflow::multicall::print_top_level_help();
            anyhow::bail!("Missing subcommand for {MULTICALL_SENTINEL}");
        };
        let argv = argv_with_program_name(cmd, it.collect());
        return gflow::multicall::dispatch(argv).await;
    }

    let argv = argv_with_program_name(first, it.collect());
    gflow::multicall::dispatch(argv).await
}

fn argv_with_program_name(program: OsString, rest: Vec<OsString>) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(rest.len() + 1);
    argv.push(program);
    argv.extend(rest);
    argv
}
