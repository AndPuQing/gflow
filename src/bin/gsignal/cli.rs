use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "gsignal",
    author,
    version,
    about = "Sends a signal to a job in the gflow scheduler."
)]
pub struct GSignal {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[arg(long, global = true, help = "Path to the config file")]
    pub config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Send finish signal to a running job
    Finish(FinishArgs),
    /// Send Fail signal to a running job
    Fail(FailArgs),
}

#[derive(Debug, Parser)]
pub struct FinishArgs {
    /// The ID of the job to finish
    pub id: u32,
}

#[derive(Debug, Parser)]
pub struct FailArgs {
    /// The ID of the job to fail
    pub id: u32,
}
