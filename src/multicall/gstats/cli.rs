use clap::Parser;
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "gstats",
    author,
    version=gflow::core::version(),
    about = "Shows usage statistics for the gflow scheduler."
)]
#[command(styles=gflow::utils::STYLES)]
pub struct GStats {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Filter by user (default: current user; use 'all' for all users)
    #[arg(long, short = 'u', value_hint = clap::ValueHint::Other)]
    pub user: Option<String>,

    /// Show stats for all users
    #[arg(long, short = 'a', conflicts_with = "user")]
    pub all_users: bool,

    /// Time range filter (e.g. '7d', '30d', '1h', 'today', or ISO timestamp)
    #[arg(long, short = 't', value_hint = clap::ValueHint::Other)]
    pub since: Option<String>,

    /// Output format (table, json, csv)
    #[arg(long, short = 'o', default_value = "table")]
    pub output: String,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate shell completion scripts
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}
