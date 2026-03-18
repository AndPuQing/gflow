use clap::Parser;
use clap_complete::Shell;
use clap_verbosity_flag::Verbosity;

#[derive(Debug, Parser)]
#[command(
    name = "mcp",
    author,
    version = gflow::build_info::version(),
    about = "Runs the local gflow MCP server."
)]
#[command(styles = gflow::utils::STYLES)]
pub struct GMcpCli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub verbosity: Verbosity,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Run the local stdio MCP server
    Serve,
    /// Generate shell completion scripts
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}
