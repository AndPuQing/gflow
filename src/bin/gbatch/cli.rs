use crate::help::COMPLETIONS_HELP;
use clap::{Parser, ValueEnum};
use clap_complete::Shell as CompleteShell;
use gflow::core::version;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "gbatch", author, version = version(), about = "Submits jobs to the gflow scheduler. Inspired by sbatch.")]
pub struct GBatch {
    #[command(subcommand)]
    pub commands: Option<Commands>,

    #[command(flatten)]
    pub add_args: AddArgs,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file")]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate tab-completion scripts for your shell
    #[command(
        after_help = COMPLETIONS_HELP,
        arg_required_else_help = true
    )]
    Completions(CompletionsArgs),
    /// Create a new job script template
    New(NewArgs),
}

#[derive(Debug, Parser)]
pub struct NewArgs {
    /// The name of the new job
    pub name: String,
}

#[derive(Debug, Parser, Clone)]
pub struct AddArgs {
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
    #[arg(short, long, name = "NUMS")]
    pub gpus: Option<u32>,

    /// The priority of the job
    #[arg(long)]
    pub priority: Option<u8>,

    /// The ID of the job this job depends on
    #[arg(long)]
    pub depends_on: Option<u32>,

    /// The job array specification (e.g., "1-10")
    #[arg(long)]
    pub array: Option<String>,
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
