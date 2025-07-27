use crate::cli::{self, GBatch};
use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use std::io;

pub(crate) fn handle_completions(args: cli::CompletionsArgs) -> Result<()> {
    let mut cmd = GBatch::command();
    let shell = args.shell;

    generate::<clap_complete::Shell, _>(
        shell.into(),
        &mut cmd,
        env!("CARGO_PKG_NAME"),
        &mut io::stdout(),
    );

    Ok(())
}
