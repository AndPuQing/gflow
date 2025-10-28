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
        short = 'n',
        help = "Limit the number of jobs to display (positive: first N, negative: last N, 0: all)",
        value_parser = clap::value_parser!(i32),
        default_value = "-10"
    )]
    pub limit: i32,

    #[arg(
        long,
        short = 'a',
        help = "Show all jobs (equivalent to --limit 0)",
        conflicts_with = "limit"
    )]
    pub all: bool,

    #[arg(
        long,
        short = 'r',
        help = "Sort jobs by field (options: id, state, time, name, gpus, priority)",
        default_value = "id"
    )]
    pub sort: String,

    #[arg(
        long,
        short = 's',
        help = "Filter by a comma-separated list of job states (e.g., Queued,Running)"
    )]
    pub states: Option<String>,

    #[arg(
        long,
        short = 'j',
        help = "Filter by a comma-separated list of job IDs"
    )]
    pub jobs: Option<String>,

    #[arg(
        long,
        short = 'N',
        help = "Filter by a comma-separated list of job names"
    )]
    pub names: Option<String>,

    #[arg(
        long,
        short = 'f',
        help = "Specify a comma-separated list of fields to display"
    )]
    pub format: Option<String>,

    #[arg(
        long,
        short = 'g',
        help = "Group jobs by state (helps visualize job distribution)"
    )]
    pub group: bool,

    #[arg(
        long,
        short = 't',
        help = "Display jobs in tree format showing dependencies"
    )]
    pub tree: bool,
}
