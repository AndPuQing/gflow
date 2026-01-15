use clap::Parser;
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "gqueue",
    author,
    version=gflow::core::version(),
    about = "Lists jobs in the gflow scheduler."
)]
#[command(styles=gflow::utils::STYLES)]
pub struct GQueue {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub list_args: ListArgs,

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

#[derive(Debug, Parser)]
pub struct ListArgs {
    #[arg(
        long,
        short = 'n',
        help = "Limit the number of jobs to display (positive: first N, negative: last N, 0: all)",
        value_parser = clap::value_parser!(i32),
        default_value = "-10",
        allow_negative_numbers = true
    )]
    pub limit: i32,

    #[arg(
        long,
        short = 'a',
        help = "Show all jobs including completed ones (equivalent to -n 0)",
        conflicts_with = "limit"
    )]
    pub all: bool,

    #[arg(
        long,
        short = 'c',
        help = "Show only completed jobs (Finished, Failed, Cancelled, Timeout)",
        conflicts_with_all = ["all", "states"]
    )]
    pub completed: bool,

    #[arg(
        long,
        help = "Show jobs since a specific time (formats: '1h', '2d', '3w', 'today', 'yesterday', or ISO timestamp)",
        value_hint = clap::ValueHint::Other
    )]
    pub since: Option<String>,

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
        help = "Filter by a comma-separated list of job states (e.g., Queued,Running)",
        value_hint = clap::ValueHint::Other
    )]
    pub states: Option<String>,

    #[arg(
        long,
        short = 'j',
        help = "Filter by a comma-separated list of job IDs",
        value_hint = clap::ValueHint::Other
    )]
    pub jobs: Option<String>,

    #[arg(
        long,
        short = 'N',
        help = "Filter by a comma-separated list of job names",
        value_hint = clap::ValueHint::Other
    )]
    pub names: Option<String>,

    #[arg(
        long,
        short = 'f',
        help = "Specify a comma-separated list of fields to display",
        value_hint = clap::ValueHint::Other
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
