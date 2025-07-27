use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "glogs",
    author,
    version,
    about = "Shows logs for a job in the gflow scheduler."
)]
pub struct GLogs {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file")]
    pub config: Option<std::path::PathBuf>,

    /// The ID of the job to show logs for
    pub id: u32,

    /// Follow the log output
    #[arg(short, long)]
    pub follow: bool,
}
