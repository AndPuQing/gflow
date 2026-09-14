use clap::Parser;
use clap_complete::Shell;
use clap_verbosity_flag::Verbosity;

#[derive(Debug, Parser)]
#[command(
    name = "ginfo",
    author,
    version=gflow::build_info::version(),
    about = "Displays gflow scheduler and GPU information."
)]
#[command(
    after_help = "BLOCKED GPUs\n  A GPU with a non-gflow compute process attached is reported as allocated\n  with an `unmanaged(pid=...)` reason. The process list below the table shows\n  each PID's memory, utilization, and age, plus the exact `gctl gpu-process\n  ignore` command that releases that GPU for scheduling.\n\n  `gctl gpu-process list` shows the ignore overrides currently in effect."
)]
#[command(styles=gflow::utils::STYLES)]
pub struct GInfoCli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub verbosity: Verbosity,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}
