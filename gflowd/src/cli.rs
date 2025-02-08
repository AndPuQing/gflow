use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "gflowd", author, version = version(), about = "GFlow Daemon")]
pub struct GFlowd {
    /// The configuration file to use]
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("VERGEN_GIT_DESCRIBE"),
    " (",
    env!("VERGEN_BUILD_DATE"),
    ")"
);

pub fn version() -> &'static str {
    let author = clap::crate_authors!();

    Box::leak(Box::new(format!(
        "\
{VERSION_MESSAGE}

Authors: {author}"
    )))
}
