use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "gctl", author, version, about = "Controls the gflow daemon.")]
pub struct GCtl {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Start the system service
    Up,
    /// Stop the system service
    Down,
    /// Show the system service status
    Status,
    /// Display system information and GPU allocation
    Info,
}
