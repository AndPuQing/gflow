use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "gcancel",
    author,
    version,
    about = "Controls job states in the gflow scheduler."
)]
pub struct GCancel {
    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,

    /// Mark job as finished (internal use)
    #[arg(long, hide = true)]
    pub finish: Option<u32>,

    /// Mark job as failed (internal use)
    #[arg(long, hide = true)]
    pub fail: Option<u32>,

    /// Job ID(s) to cancel. Supports ranges like "1-3" or individual IDs like "1,2,3"
    pub ids: Option<String>,

    /// If set, the job will not be cancelled, but the action will be printed
    #[arg(long)]
    pub dry_run: bool,
}
