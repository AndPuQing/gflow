use std::path::PathBuf;

use clap::Parser;
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(name = "gflowd", author, version = gflow::core::version(), about = "GFlow Daemon")]
#[command(styles=gflow::utils::STYLES)]
pub struct GFlowd {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// The configuration file to use
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Clean up the configuration file
    #[arg(long, global = true)]
    pub cleanup: bool,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Start the daemon in a tmux session
    Up,
    /// Stop the daemon
    Down,
    /// Restart the daemon
    Restart,
    /// Show the daemon status
    Status,
    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}
