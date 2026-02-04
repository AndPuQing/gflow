use clap::Parser;
use clap_complete::Shell;
use gflow::core::version;

#[derive(Debug, Parser)]
#[command(name = "gbatch", author, version = version(), about = "Submits jobs to the gflow scheduler. Inspired by sbatch.")]
#[command(styles=gflow::utils::STYLES)]
pub struct GBatch {
    #[command(subcommand)]
    pub commands: Option<Commands>,

    #[command(flatten)]
    pub add_args: AddArgs,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Create a new job script template
    New(NewArgs),
    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Parser)]
pub struct NewArgs {
    /// The name of the new job
    pub name: String,
}

#[derive(Debug, Parser, Clone)]
pub struct AddArgs {
    /// The script or command to run (e.g., "script.sh" or "python train.py --epochs 100")
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_hint = clap::ValueHint::CommandWithArguments)]
    pub script_or_command: Vec<String>,

    /// The conda environment to use
    #[arg(short, long, value_hint = clap::ValueHint::Other)]
    pub conda_env: Option<String>,

    /// The GPU count to request
    #[arg(short, long, visible_alias = "gres", name = "NUMS")]
    pub gpus: Option<u32>,

    /// The priority of the job
    #[arg(short = 'p', long, visible_alias = "nice")]
    pub priority: Option<u8>,

    /// Job dependency; accepts a job ID or shorthand like "@" / "@~N"
    #[arg(short = 'd', long, visible_alias = "dependency", value_hint = clap::ValueHint::Other)]
    pub depends_on: Option<String>,

    /// Multiple job dependencies with AND logic (all must finish successfully)
    /// Accepts comma-separated job IDs or shorthands: "123,456,@"
    #[arg(long, value_hint = clap::ValueHint::Other, conflicts_with = "depends_on")]
    pub depends_on_all: Option<String>,

    /// Multiple job dependencies with OR logic (any one must finish successfully)
    /// Accepts comma-separated job IDs or shorthands: "123,456,@"
    #[arg(long, value_hint = clap::ValueHint::Other, conflicts_with_all = ["depends_on", "depends_on_all"])]
    pub depends_on_any: Option<String>,

    /// Disable auto-cancellation when dependency fails (default: enabled)
    #[arg(long)]
    pub no_auto_cancel: bool,

    /// The job array specification (e.g., "1-10")
    #[arg(long, value_hint = clap::ValueHint::Other)]
    pub array: Option<String>,

    /// Time limit for the job (formats: "HH:MM:SS", "MM:SS", "MM", or seconds as number)
    #[arg(
        short = 't',
        long,
        visible_aliases = ["time-limit", "timelimit"],
        value_hint = clap::ValueHint::Other
    )]
    pub time: Option<String>,

    /// Memory limit for the job (formats: "100G", "1024M", or "512" for MB)
    #[arg(short = 'm', long, value_hint = clap::ValueHint::Other)]
    pub memory: Option<String>,

    /// Custom run name for the job (used as tmux session name)
    #[arg(
        short = 'n',
        short_alias = 'J',
        long,
        visible_alias = "job-name",
        value_hint = clap::ValueHint::Other
    )]
    pub name: Option<String>,

    /// Automatically close tmux session on successful completion
    #[arg(long)]
    pub auto_close: bool,

    /// Parameter specification (e.g., "scale=2.0,1.9,1.8")
    /// Can be specified multiple times for cartesian product
    #[arg(long, value_hint = clap::ValueHint::Other)]
    pub param: Vec<String>,

    /// Preview what would be submitted without actually submitting
    #[arg(long)]
    pub dry_run: bool,

    /// Maximum number of jobs from this submission that can run concurrently
    #[arg(long, value_hint = clap::ValueHint::Other)]
    pub max_concurrent: Option<usize>,

    /// Load parameters from a CSV file (header row required)
    #[arg(long, value_hint = clap::ValueHint::FilePath)]
    pub param_file: Option<std::path::PathBuf>,

    /// Template for job names when using --param or --param-file
    /// Use {param_name} to substitute parameter values
    #[arg(long, value_hint = clap::ValueHint::Other)]
    pub name_template: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slurm_compatible_aliases() {
        let args = GBatch::try_parse_from([
            "gbatch",
            "--time-limit",
            "2:00:00",
            "--nice",
            "10",
            "--job-name",
            "train",
            "--gres",
            "2",
            "--dependency",
            "@",
            "script.sh",
        ])
        .expect("should parse SLURM-compatible aliases");

        assert_eq!(args.add_args.time.as_deref(), Some("2:00:00"));
        assert_eq!(args.add_args.priority, Some(10));
        assert_eq!(args.add_args.name.as_deref(), Some("train"));
        assert_eq!(args.add_args.gpus, Some(2));
        assert_eq!(args.add_args.depends_on.as_deref(), Some("@"));
        assert_eq!(
            args.add_args.script_or_command,
            vec!["script.sh".to_string()]
        );
    }
}
