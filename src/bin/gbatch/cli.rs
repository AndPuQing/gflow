use clap::Parser;
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
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    pub script_or_command: Vec<String>,

    #[arg(short, long)]
    /// The conda environment to use
    pub conda_env: Option<String>,

    /// The GPU count to request
    #[arg(short, long, name = "NUMS")]
    pub gpus: Option<u32>,

    /// The priority of the job
    #[arg(long)]
    pub priority: Option<u8>,

    /// The ID of the job this job depends on
    #[arg(long)]
    pub depends_on: Option<u32>,

    /// The job array specification (e.g., "1-10")
    #[arg(long)]
    pub array: Option<String>,

    /// Time limit for the job (formats: "HH:MM:SS", "MM:SS", "MM", or seconds as number)
    #[arg(short = 't', long)]
    pub time: Option<String>,
}
