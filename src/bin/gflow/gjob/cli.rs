use clap::Parser;
use clap_complete::Shell;
use clap_verbosity_flag::Verbosity;
use gflow::utils::STYLES;

#[derive(Debug, Parser)]
#[command(
    name = "gjob",
    author,
    version=gflow::core::version(),
    about = "Controls and manages jobs in the gflow scheduler.",
)]
#[command(styles=STYLES)]
pub struct GJob {
    #[command(flatten)]
    pub verbosity: Verbosity,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]

pub enum Commands {
    /// Attach to a job's tmux session
    #[command(visible_alias = "a")]
    Attach {
        #[arg(help = "Job ID to attach to (supports @ for most recent job)", value_hint = clap::ValueHint::Other)]
        job: String,
    },
    /// View a job's log output
    #[command(visible_alias = "l")]
    Log {
        #[arg(help = "Job ID to view the log for (supports @ for most recent job)", value_hint = clap::ValueHint::Other)]
        job: String,
    },
    /// Put a queued job on hold
    #[command(visible_alias = "h")]
    Hold {
        #[arg(
            help = "Job ID(s) to hold. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\"",
            value_hint = clap::ValueHint::Other
        )]
        job: String,
    },
    /// Release a held job back to the queue
    #[command(visible_alias = "r")]
    Release {
        #[arg(
            help = "Job ID(s) to release. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\"",
            value_hint = clap::ValueHint::Other
        )]
        job: String,
    },
    /// Update parameters for a queued or held job
    #[command(visible_alias = "u")]
    Update {
        #[arg(
            help = "Job ID(s) to update. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\"",
            value_hint = clap::ValueHint::Other
        )]
        job: String,

        #[arg(short = 'c', long, help = "Update command", value_hint = clap::ValueHint::Other)]
        command: Option<String>,

        #[arg(short = 's', long, help = "Update script path", value_hint = clap::ValueHint::FilePath)]
        script: Option<std::path::PathBuf>,

        #[arg(short = 'g', long, help = "Update number of GPUs")]
        gpus: Option<u32>,

        #[arg(short = 'e', long, help = "Update conda environment", value_hint = clap::ValueHint::Other)]
        conda_env: Option<String>,

        #[arg(long, help = "Clear conda environment")]
        clear_conda_env: bool,

        #[arg(short = 'p', long, help = "Update priority (0-255)")]
        priority: Option<u8>,

        #[arg(short = 't', long, help = "Update time limit (formats: HH:MM:SS, MM:SS, or MM)", value_hint = clap::ValueHint::Other)]
        time_limit: Option<String>,

        #[arg(long, help = "Clear time limit")]
        clear_time_limit: bool,

        #[arg(short = 'm', long, help = "Update memory limit (formats: 100G, 1024M, or 512 for MB)", value_hint = clap::ValueHint::Other)]
        memory_limit: Option<String>,

        #[arg(long, help = "Clear memory limit")]
        clear_memory_limit: bool,

        #[arg(
            short = 'd',
            long,
            help = "Update dependencies (comma-separated job IDs)",
            value_delimiter = ','
        )]
        depends_on: Option<Vec<u32>>,

        #[arg(
            long,
            help = "Update dependencies with AND logic (all must finish)",
            value_delimiter = ','
        )]
        depends_on_all: Option<Vec<u32>>,

        #[arg(
            long,
            help = "Update dependencies with OR logic (any one must finish)",
            value_delimiter = ','
        )]
        depends_on_any: Option<Vec<u32>>,

        #[arg(long, help = "Enable auto-cancel when dependency fails")]
        auto_cancel_on_dep_failure: bool,

        #[arg(long, help = "Disable auto-cancel when dependency fails")]
        no_auto_cancel_on_dep_failure: bool,

        #[arg(long, help = "Update max concurrent jobs in group")]
        max_concurrent: Option<usize>,

        #[arg(long, help = "Clear max concurrent limit")]
        clear_max_concurrent: bool,

        #[arg(long = "param", help = "Update parameter (KEY=VALUE, can be repeated)", value_hint = clap::ValueHint::Other)]
        params: Vec<String>,
    },
    /// Show detailed information about a job
    #[command(visible_alias = "s")]
    Show {
        #[arg(
            help = "Job ID(s) to show details for. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\"",
            value_hint = clap::ValueHint::Other
        )]
        job: String,
    },
    /// Resubmit a job with the same or modified parameters
    Redo {
        #[arg(help = "Job ID to resubmit (supports @ for most recent job)", value_hint = clap::ValueHint::Other)]
        job: String,

        #[arg(short, long, help = "Override number of GPUs")]
        gpus: Option<u32>,

        #[arg(short, long, help = "Override priority")]
        priority: Option<u8>,

        #[arg(short = 'd', long, help = "Override or set dependency (job ID or @)", value_hint = clap::ValueHint::Other)]
        depends_on: Option<String>,

        #[arg(
            short,
            long,
            help = "Override time limit (formats: HH:MM:SS, MM:SS, or MM)",
            value_hint = clap::ValueHint::Other
        )]
        time: Option<String>,

        #[arg(
            short = 'm',
            long,
            help = "Override memory limit (formats: 100G, 1024M, or 512 for MB)",
            value_hint = clap::ValueHint::Other
        )]
        memory: Option<String>,

        #[arg(short = 'e', long, help = "Override conda environment", value_hint = clap::ValueHint::Other)]
        conda_env: Option<String>,

        #[arg(long, help = "Clear dependency from original job")]
        clear_deps: bool,

        #[arg(
            long,
            help = "Also redo dependent jobs that were cancelled due to this job's failure"
        )]
        cascade: bool,
    },
    /// Close tmux sessions for completed jobs (by default). Use --state to close sessions in other states.
    #[command(visible_alias = "close")]
    CloseSessions {
        #[arg(
            short = 'j',
            long,
            help = "Job ID(s) to close sessions for. Supports ranges like \"1-3\" or individual IDs like \"1,2,3\"",
            value_hint = clap::ValueHint::Other
        )]
        jobs: Option<String>,

        #[arg(
            short = 's',
            long,
            help = "Close sessions for jobs in specific state(s). Accepts: queued, hold, running, finished, failed, cancelled, timeout",
            value_delimiter = ',',
            value_hint = clap::ValueHint::Other
        )]
        state: Option<Vec<gflow::core::job::JobState>>,

        #[arg(
            short = 'p',
            long,
            help = "Close sessions matching this pattern (substring match on session name)",
            value_hint = clap::ValueHint::Other
        )]
        pattern: Option<String>,

        #[arg(
            short = 'a',
            long,
            help = "Close sessions for all completed jobs (finished, failed, cancelled, timeout)"
        )]
        all: bool,
    },
    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}
