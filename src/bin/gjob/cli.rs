use clap::Parser;

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
        #[arg(short, long, help = "Job ID to hold")]
        job: u32,
    },
    /// Release a held job back to the queue
    #[command(visible_alias = "r")]
    Release {
        #[arg(short, long, help = "Job ID to release")]
        job: u32,
    },
    /// Show detailed information about a job
    #[command(visible_alias = "s")]
    Show {
        #[arg(short, long, help = "Job ID to show details for")]
        job: u32,
    },
}
