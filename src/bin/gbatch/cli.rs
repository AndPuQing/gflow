use clap::Parser;
use clap_complete::Shell;
use gflow::core::version;

#[derive(Debug, Parser)]
#[command(name = "gbatch", author, version = version(), about = "Submits jobs to the gflow scheduler. Inspired by sbatch.")]
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
    /// If a single argument that exists as a file, it's treated as a script.
    /// Otherwise, all arguments are joined as a command.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_hint = clap::ValueHint::CommandWithArguments)]
    pub script_or_command: Vec<String>,

    /// The conda environment to use
    #[arg(short, long, value_hint = clap::ValueHint::Other)]
    pub conda_env: Option<String>,

    /// The GPU count to request
    #[arg(short, long, name = "NUMS")]
    pub gpus: Option<u32>,

    /// The priority of the job
    #[arg(long)]
    pub priority: Option<u8>,

    /// Job dependency; accepts a job ID or shorthand like "@" / "@~N"
    #[arg(long, value_hint = clap::ValueHint::Other)]
    pub depends_on: Option<String>,

    /// The job array specification (e.g., "1-10")
    #[arg(long, value_hint = clap::ValueHint::Other)]
    pub array: Option<String>,

    /// Time limit for the job (formats: "HH:MM:SS", "MM:SS", "MM", or seconds as number)
    #[arg(short = 't', long, value_hint = clap::ValueHint::Other)]
    pub time: Option<String>,

    /// Custom run name for the job (used as tmux session name)
    #[arg(short = 'n', long, value_hint = clap::ValueHint::Other)]
    pub name: Option<String>,
}
