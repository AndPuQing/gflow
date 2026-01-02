use clap::Parser;
use clap_complete::Shell;
use clap_verbosity_flag::Verbosity;

#[derive(Debug, Parser)]
#[command(
    name = "ginfo",
    author,
    version=gflow::core::version(),
    about = "Displays gflow scheduler and GPU information."
)]
#[command(styles=gflow::utils::STYLES)]
pub struct GInfoCli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub verbosity: Verbosity,

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
