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

    /// Manage GPU reservations
    Reserve {
        #[command(subcommand)]
        command: ReserveCommands,
    },

    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Parser)]
pub enum ReserveCommands {
    /// Create a GPU reservation
    Create {
        /// Username for the reservation
        #[arg(long)]
        user: String,
        /// Number of GPUs to reserve
        #[arg(long)]
        gpus: u32,
        /// Start time (ISO8601 format or "YYYY-MM-DD HH:MM")
        #[arg(long)]
        start: String,
        /// Duration (e.g., "1h", "30m", "2h30m")
        #[arg(long)]
        duration: String,
    },

    /// List GPU reservations
    List {
        /// Filter by username
        #[arg(long)]
        user: Option<String>,
        /// Filter by status (pending, active, completed, cancelled)
        #[arg(long)]
        status: Option<String>,
        /// Show only active reservations
        #[arg(long)]
        active: bool,
    },

    /// Get details of a specific reservation
    Get {
        /// Reservation ID
        id: u32,
    },

    /// Cancel a GPU reservation
    Cancel {
        /// Reservation ID
        id: u32,
    },
}
