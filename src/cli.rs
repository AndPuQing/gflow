use crate::help::COMPLETIONS_HELP;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
}

#[derive(Debug, Parser)]
#[command(name = "gflow", author, version = version(), about = "A tiny job scheduler inspired by Slurm.")]
pub struct GFlow {
    /// Sub Commands
    #[command(subcommand)]
    pub commands: Option<Commands>,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Add a new job to the scheduler
    Add(AddArgs),
    /// Generate tab-completion scripts for your shell
    #[command(
        after_help = COMPLETIONS_HELP,
        arg_required_else_help = true
    )]
    Completions(CompletionsArgs),
}

#[derive(Debug, Parser)]
pub struct AddArgs {
    /// The script to run
    pub script: PathBuf,
    /// The GPU count to request
    #[clap(short, long, name = "NUMS", default_value = "0")]
    pub gpus: Option<u32>,
}

#[derive(Debug, Parser)]
pub struct CompletionsArgs {
    /// The shell to generate the completions for
    pub shell: Shell,
}

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("VERGEN_GIT_DESCRIBE"),
    " (",
    env!("VERGEN_BUILD_DATE"),
    ")"
);

pub fn version() -> &'static str {
    let author = clap::crate_authors!();

    Box::leak(Box::new(format!(
        "\
{VERSION_MESSAGE}

Authors: {author}"
    )))
}
