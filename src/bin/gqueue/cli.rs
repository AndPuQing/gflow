use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "gqueue",
    author,
    version,
    about = "Lists jobs in the gflow scheduler."
)]
pub struct GQueue {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file")]
    pub config: Option<std::path::PathBuf>,
}
