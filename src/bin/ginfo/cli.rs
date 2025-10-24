use clap::Parser;
use gflow::core::version;

#[derive(Debug, Parser)]
#[command(name = "ginfo", author, version = version(), about = "Display information about gflow resources.")]
pub struct GInfo {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, global = true, help = "Path to the config file", hide = true)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Display partition information
    Partition(PartitionArgs),
    /// Display GPU information
    Gpu(GpuArgs),
}

#[derive(Debug, Parser)]
pub struct PartitionArgs {
    // I might add arguments later, like a specific partition name.
}

#[derive(Debug, Parser)]
pub struct GpuArgs {
    // I might add arguments later, like a specific GPU id.
}
