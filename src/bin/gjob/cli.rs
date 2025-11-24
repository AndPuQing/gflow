use clap::Parser;
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "gjob",
    author,
    version=gflow::core::version(),
    about = "Controls and manages jobs in the gflow scheduler."
)]
pub struct GJob {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]

pub enum Commands {
    /// Attach to a job's tmux session
    #[command(visible_alias = "a")]
    Attach {
        #[arg(short, long, help = "Job ID to attach to")]
        job: u32,
    },
    /// View a job's log output
    #[command(visible_alias = "l")]
    Log {
        #[arg(short, long, help = "Job ID to view the log for")]
        job: u32,
    },
    /// Put a queued job on hold
    #[command(visible_alias = "h")]
    Hold {
        #[arg(
            short,
            long,
            help = "Job ID(s) to hold. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\""
        )]
        job: String,
    },
    /// Release a held job back to the queue
    #[command(visible_alias = "r")]
    Release {
        #[arg(
            short,
            long,
            help = "Job ID(s) to release. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\""
        )]
        job: String,
    },
    /// Show detailed information about a job
    #[command(visible_alias = "s")]
    Show {
        #[arg(
            short,
            long,
            help = "Job ID(s) to show details for. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\""
        )]
        job: String,
    },
    /// Resubmit a job with the same or modified parameters
    Redo {
        #[arg(help = "Job ID to resubmit (supports @ for most recent job)")]
        job: String,

        #[arg(short, long, help = "Override number of GPUs")]
        gpus: Option<u32>,

        #[arg(short, long, help = "Override priority")]
        priority: Option<u8>,

        #[arg(short = 'd', long, help = "Override or set dependency (job ID or @)")]
        depends_on: Option<String>,

        #[arg(
            short,
            long,
            help = "Override time limit (formats: HH:MM:SS, MM:SS, or MM)"
        )]
        time: Option<String>,

        #[arg(short = 'e', long, help = "Override conda environment")]
        conda_env: Option<String>,

        #[arg(long, help = "Clear dependency from original job")]
        clear_deps: bool,
    },
    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}
