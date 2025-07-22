use crate::help::COMPLETIONS_HELP;
use clap::{Parser, ValueEnum};
use clap_complete::Shell as CompleteShell;
use gflow_core::version;
use std::path::PathBuf;

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
    Submit(SubmitArgs),
    /// List all jobs in the scheduler
    List,
    /// Start the system service
    Up,
    /// Stop the system service
    Stop,
    /// Generate tab-completion scripts for your shell
    #[command(
        after_help = COMPLETIONS_HELP,
        arg_required_else_help = true
    )]
    Completions(CompletionsArgs),
    /// Send finish signal to a running job
    Finish(FinishArgs),
    /// Send Fail signal to a running job
    Fail(FailArgs),
}

#[derive(Debug, Parser)]
pub struct FinishArgs {
    /// The name of the job to finish
    pub name: String,
}

#[derive(Debug, Parser)]
pub struct FailArgs {
    /// The name of the job to fail
    pub name: String,
}

#[derive(Debug, Parser)]
pub struct SubmitArgs {
    /// The script to run
    #[arg(required_unless_present = "command")]
    pub script: Option<PathBuf>,

    /// The command to run
    #[arg(long, conflicts_with = "script")]
    pub command: Option<String>,

    #[arg(short, long)]
    /// The conda environment to use
    pub conda_env: Option<String>,

    /// The GPU count to request
    #[arg(short, long, name = "NUMS", default_value = "0")]
    pub gpus: Option<u32>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
}

#[derive(Debug, Parser)]
pub struct CompletionsArgs {
    /// The shell to generate the completions for
    pub shell: Shell,
}

impl From<Shell> for CompleteShell {
    fn from(shell: Shell) -> Self {
        match shell {
            Shell::Bash => CompleteShell::Bash,
            Shell::Elvish => CompleteShell::Elvish,
            Shell::Fish => CompleteShell::Fish,
            Shell::Powershell => CompleteShell::PowerShell,
            Shell::Zsh => CompleteShell::Zsh,
        }
    }
}
