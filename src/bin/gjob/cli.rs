use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "gjob",
    author,
    version,
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
}
