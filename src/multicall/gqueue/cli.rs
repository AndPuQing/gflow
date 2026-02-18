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
        short = 'u',
        visible_alias = "users",
        help = "Filter by a comma-separated list of users (default: current user; use 'all' to show all users)",
        value_hint = clap::ValueHint::Other
    )]
    pub user: Option<String>,

    #[arg(
        long,
        short = 'n',
        help = "Limit the number of jobs to display (positive: first N, negative: last N, 0: all)",
        value_parser = clap::value_parser!(i32),
        default_value = "0",
        allow_negative_numbers = true
    )]
    pub limit: i32,

    #[arg(long, short = 'a', help = "Show all jobs including completed ones")]
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
        visible_alias = "job",
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

    #[arg(long, short = 'T', help = "Show only jobs with active tmux sessions")]
    pub tmux: bool,

    #[arg(
        long,
        short = 'o',
        help = "Output format (options: table, json, csv, yaml)",
        default_value = "table"
    )]
    pub output: String,

    #[arg(long, short = 'w', help = "Auto-refresh job list (default: every 2s)")]
    pub watch: bool,

    #[arg(
        long,
        help = "Refresh interval in seconds for --watch mode",
        default_value = "2",
        requires = "watch"
    )]
    pub interval: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slurm_style_user_and_job_aliases() {
        let args = GQueue::try_parse_from(["gqueue", "--users", "alice,bob", "--job", "1,2,3"])
            .expect("should parse aliases");

        assert_eq!(args.list_args.user.as_deref(), Some("alice,bob"));
        assert_eq!(args.list_args.jobs.as_deref(), Some("1,2,3"));
    }
}
