use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "gflowd", author, version = gflow::version(), about = "GFlow Daemon")]
pub struct GFlowd {
    /// The configuration file to use]
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Clean up the configuration file
    #[arg(long)]
    pub cleanup: bool,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}
