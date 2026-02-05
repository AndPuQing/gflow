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
        /// Number of GPUs to reserve (mutually exclusive with --gpu-spec)
        #[arg(long, conflicts_with = "gpu_spec")]
        gpus: Option<u32>,
        /// GPU specification (e.g., "0,2" or "0-3" or "0-1,3,5-6")
        /// Mutually exclusive with --gpus
        #[arg(long, conflicts_with = "gpus")]
        gpu_spec: Option<String>,
        /// Start time (ISO8601 format or "YYYY-MM-DD HH:MM")
        #[arg(long)]
        start: String,
        /// Duration (e.g., "1h", "30m", "2h30m")
        #[arg(long)]
        duration: String,
        /// Timezone for interpreting start time (e.g., "Asia/Shanghai", "UTC")
        /// Overrides config file timezone setting
        #[arg(long)]
        timezone: Option<String>,
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
        /// Display as timeline visualization
        #[arg(long)]
        timeline: bool,
        /// Timeline time range (relative to now). Formats:
        /// - "48h" (now..now+48h)
        /// - "-24h" (now-24h..now)
        /// - "-24h:+24h" (now-24h..now+24h)
        #[arg(
            long,
            value_name = "RANGE",
            requires = "timeline",
            conflicts_with_all = ["from", "to"]
        )]
        range: Option<String>,
        /// Timeline range start time (ISO8601 or "YYYY-MM-DD HH:MM").
        #[arg(
            long,
            value_name = "TIME",
            requires_all = ["timeline", "to"],
            conflicts_with = "range"
        )]
        from: Option<String>,
        /// Timeline range end time (ISO8601 or "YYYY-MM-DD HH:MM").
        #[arg(
            long,
            value_name = "TIME",
            requires_all = ["timeline", "from"],
            conflicts_with = "range"
        )]
        to: Option<String>,
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
