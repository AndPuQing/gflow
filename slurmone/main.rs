mod commands;
mod common;
mod slurm;
mod slurmoned;
use commands::handle_exec;
use common::arg::JobArgs;
use std::path::Path;
use std::{error::Error, str::FromStr};

use common::config::Config;
use tracing::Level;
use tracing_subscriber::fmt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let clargs = JobArgs::new();
    let config: Config = Config::init(None)?;
    let log_path = config
        .slurmone
        .log_dir
        .clone()
        .unwrap_or_else(|| "./logs".to_string());
    let path: &Path = Path::new(&log_path);
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }

    let level = config
        .slurmone
        .log_level
        .clone()
        .unwrap_or_else(|| "info".to_string());

    let appender = tracing_appender::rolling::daily(&log_path, "slurmone.log");
    let (non_blocking_appender, _guard) = tracing_appender::non_blocking(appender);
    let subscriber = fmt::Subscriber::builder()
        .with_writer(non_blocking_appender)
        .with_max_level(Level::from_str(&level)?)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set tracing default subscriber");

    if let Some(commands) = clargs.commands {
        let _res = handle_exec(commands, config).await;
    }
    return Ok(());
}
