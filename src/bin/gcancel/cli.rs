use clap::Parser;
use clap_complete::Shell;
use gflow::core::version;

#[derive(Debug, Parser)]
#[command(
    name = "gcancel",
    author,
    version=version(),
    about = "Controls job states in the gflow scheduler."
)]
#[command(styles=gflow::utils::STYLES)]
pub struct GCancel {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub cancel_args: CancelArgs,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate shell completion scripts
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Parser)]
pub struct CancelArgs {
    /// Mark job as finished (internal use)
    #[arg(long, hide = true)]
    pub finish: Option<u32>,

    /// Mark job as failed (internal use)
    #[arg(long, hide = true)]
    pub fail: Option<u32>,

    /// Job ID(s) to cancel. Supports ranges like "1-3" or individual IDs like "1,2,3"
    #[arg(value_hint = clap::ValueHint::Other)]
    pub ids: Option<String>,

    /// If set, the job will not be cancelled, but the action will be printed
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug)]
pub enum CancelCommand {
    Cancel { ids: String, dry_run: bool },
    Finish { id: u32 },
    Fail { id: u32 },
}

impl CancelArgs {
    pub fn get_command(&self) -> anyhow::Result<CancelCommand> {
        if let Some(job_id) = self.finish {
            Ok(CancelCommand::Finish { id: job_id })
        } else if let Some(job_id) = self.fail {
            Ok(CancelCommand::Fail { id: job_id })
        } else if let Some(ref ids) = self.ids {
            Ok(CancelCommand::Cancel {
                ids: ids.clone(),
                dry_run: self.dry_run,
            })
        } else {
            anyhow::bail!("No command specified. Use --finish <id>, --fail <id>, or provide job IDs to cancel")
        }
    }
}
