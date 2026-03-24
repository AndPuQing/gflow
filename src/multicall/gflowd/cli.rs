use std::path::PathBuf;

use clap::Parser;
use clap_complete::Shell;
use clap_verbosity_flag::Verbosity;

#[derive(Debug, Parser)]
#[command(name = "gflowd", author, version = gflow::build_info::version(), about = "GFlow Daemon")]
#[command(styles=gflow::utils::STYLES)]
pub struct GFlowd {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// The configuration file to use
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Clean up the configuration file
    #[arg(long, global = true)]
    pub cleanup: bool,

    /// GPU indices restriction (internal use, set by 'gflowd up --gpus')
    #[arg(long, hide = true)]
    pub gpus_internal: Option<String>,

    /// GPU allocation strategy override (internal use, set by 'gflowd up --gpu-allocation-strategy')
    #[arg(long, hide = true)]
    pub gpu_allocation_strategy_internal: Option<String>,

    /// GPU poll interval override (internal use, set by 'gflowd up --gpu-poll-interval-secs')
    #[arg(long, hide = true)]
    pub gpu_poll_interval_secs_internal: Option<u64>,

    #[command(flatten)]
    pub verbosity: Verbosity,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Create or update the configuration file via a guided wizard
    Init {
        /// Accept all defaults (non-interactive)
        #[arg(long)]
        yes: bool,

        /// Overwrite existing configuration file
        #[arg(long)]
        force: bool,

        /// Configure advanced options (notifications, etc.)
        #[arg(long)]
        advanced: bool,

        /// Limit which GPUs the scheduler can use (e.g., "0,2" or "0-2")
        #[arg(long, value_name = "INDICES")]
        gpus: Option<String>,

        /// Daemon host (default: localhost)
        #[arg(long, value_name = "HOST")]
        host: Option<String>,

        /// Daemon port (default: 59000)
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,

        /// Timezone to store in config (e.g., "Asia/Shanghai", "UTC"). Use "local" to leave unset.
        #[arg(long, value_name = "TZ")]
        timezone: Option<String>,

        /// GPU allocation strategy: sequential or random
        #[arg(long, value_name = "STRATEGY")]
        gpu_allocation_strategy: Option<String>,

        /// Poll interval in seconds for GPU occupancy detection (default: 10)
        #[arg(long, value_name = "SECONDS")]
        gpu_poll_interval_secs: Option<u64>,
    },
    /// Start the daemon in a tmux session
    Up {
        /// Limit which GPUs the scheduler can use (e.g., "0,2" or "0-2")
        #[arg(long, value_name = "INDICES")]
        gpus: Option<String>,

        /// GPU allocation strategy: sequential or random
        #[arg(long, value_name = "STRATEGY")]
        gpu_allocation_strategy: Option<String>,

        /// Poll interval in seconds for GPU occupancy detection (default: 10)
        #[arg(long, value_name = "SECONDS")]
        gpu_poll_interval_secs: Option<u64>,
    },
    /// Stop the daemon
    Down,
    /// Restart the daemon
    Restart {
        /// Limit which GPUs the scheduler can use (e.g., "0,2" or "0-2")
        #[arg(long, value_name = "INDICES")]
        gpus: Option<String>,

        /// GPU allocation strategy: sequential or random
        #[arg(long, value_name = "STRATEGY")]
        gpu_allocation_strategy: Option<String>,

        /// Poll interval in seconds for GPU occupancy detection (default: 10)
        #[arg(long, value_name = "SECONDS")]
        gpu_poll_interval_secs: Option<u64>,
    },
    /// Reload the daemon with zero downtime
    Reload {
        /// Limit which GPUs the scheduler can use (e.g., "0,2" or "0-2")
        #[arg(long, value_name = "INDICES")]
        gpus: Option<String>,

        /// GPU allocation strategy: sequential or random
        #[arg(long, value_name = "STRATEGY")]
        gpu_allocation_strategy: Option<String>,

        /// Poll interval in seconds for GPU occupancy detection (default: 10)
        #[arg(long, value_name = "SECONDS")]
        gpu_poll_interval_secs: Option<u64>,
    },
    /// Show the daemon status
    Status,
    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}
