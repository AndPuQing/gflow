use crate::help::COMPLETIONS_HELP;
use clap::{Parser, ValueEnum};
use clap_complete::Shell as CompleteShell;
use gflow::core::version;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "gflow", author, version = version(), about = "A tiny job scheduler inspired by Slurm.")]
pub struct GFlow {
    /// Sub Commands
    #[command(subcommand)]
    pub commands: Option<Commands>,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file")]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Add a new job to the scheduler
    #[command(alias = "submit")]
    Add(AddArgs),
    /// List all jobs in the scheduler
    #[command(alias = "ls")]
    List(ListArgs),
    /// Manage the system service
    #[command(subcommand)]
    Daemon(DaemonCommands),
    /// Generate tab-completion scripts for your shell
    #[command(
        after_help = COMPLETIONS_HELP,
        arg_required_else_help = true
    )]
    Completions(CompletionsArgs),
    /// Manage jobs
    #[command(subcommand)]
    Job(JobCommands),
    /// Create a new job script template
    New(NewArgs),
}

#[derive(Debug, Parser)]
pub enum JobCommands {
    /// Send finish signal to a running job
    Finish(FinishArgs),
    /// Send Fail signal to a running job
    Fail(FailArgs),
    /// Show the logs of a job
    Logs(LogsArgs),
}

#[derive(Debug, Parser)]
pub enum DaemonCommands {
    /// Start the system service
    Start,
    /// Stop the system service
    Stop,
    /// Show the system service status
    Status,
}

#[derive(Debug, Parser)]
pub struct ListArgs {
    /// Show the TUI
    #[arg(long)]
    pub tui: bool,
}

#[derive(Debug, Parser)]
pub struct FinishArgs {
    /// The ID of the job to finish
    pub id: u32,
}

#[derive(Debug, Parser)]
pub struct FailArgs {
    /// The ID of the job to fail
    pub id: u32,
}

#[derive(Debug, Parser)]
pub struct LogsArgs {
    /// The ID of the job to show logs for
    pub id: u32,
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
