use clap::Parser;
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "gctl",
    author,
    version = gflow::core::version(),
    about = "Control gflow scheduler at runtime"
)]
#[command(styles = gflow::utils::STYLES)]
pub struct GCtl {
    #[command(subcommand)]
    pub command: Commands,

    /// Path to the config file
    #[arg(long, global = true, hide = true)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Set which GPUs the scheduler can use
    SetGpus {
        /// GPU indices (e.g., "0,2" or "0-2"), or "all" for all GPUs
        gpu_spec: String,
    },

    /// Show current GPU configuration
    ShowGpus,

    /// Set concurrency limit for a job group
    SetLimit {
        /// Job ID (any job in the group) or Group ID (UUID)
        job_or_group_id: String,
        /// Maximum number of concurrent jobs in the group
        limit: usize,
    },

    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}
