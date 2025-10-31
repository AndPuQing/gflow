use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "ginfo",
    author,
    version,
    about = "Displays gflow scheduler and GPU information."
)]
pub struct GInfoCli {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Display system information and GPU allocation
    Info,
}
