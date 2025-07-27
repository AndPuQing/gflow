use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "gqueue",
    author,
    version,
    about = "Lists jobs in the gflow scheduler."
)]
pub struct GQueue {
    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,

    #[arg(
        long,
        short,
        help = "Filter by a comma-separated list of job states (e.g., Queued,Running)"
    )]
    pub states: Option<String>,

    #[arg(long, short, help = "Filter by a comma-separated list of job IDs")]
    pub jobs: Option<String>,

    #[arg(long, short, help = "Filter by a comma-separated list of job names")]
    pub names: Option<String>,

    #[arg(
        long,
        short,
        help = "Specify a comma-separated list of fields to display"
    )]
    pub format: Option<String>,
}
