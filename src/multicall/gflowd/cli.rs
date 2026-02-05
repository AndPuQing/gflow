use std::path::PathBuf;

use clap::Parser;
use clap_complete::Shell;
use clap_verbosity_flag::Verbosity;

#[derive(Debug, Parser)]
#[command(name = "gflowd", author, version = gflow::core::version(), about = "GFlow Daemon")]
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
    },
    /// Start the daemon in a tmux session
    Up {
        /// Limit which GPUs the scheduler can use (e.g., "0,2" or "0-2")
        #[arg(long, value_name = "INDICES")]
        gpus: Option<String>,
    },
    /// Stop the daemon
    Down,
    /// Restart the daemon
    Restart {
        /// Limit which GPUs the scheduler can use (e.g., "0,2" or "0-2")
        #[arg(long, value_name = "INDICES")]
        gpus: Option<String>,
    },
    /// Reload the daemon with zero downtime
    Reload {
        /// Limit which GPUs the scheduler can use (e.g., "0,2" or "0-2")
        #[arg(long, value_name = "INDICES")]
        gpus: Option<String>,
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
