use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "gcancel",
    author,
    version,
    about = "Cancels a job in the gflow scheduler."
)]
pub struct GCancel {
    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,

    /// The ID of the job to cancel
    pub id: u32,

    /// If set, the job will not be cancelled, but the action will be printed
    #[arg(long)]
    pub dry_run: bool,
}
