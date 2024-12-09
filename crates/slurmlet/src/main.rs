use std::error::Error;
pub mod commands;
use commands::handle_exec;
use common::arg::JobArgs;
use std::path::Path;

use common::config::Config;
use tracing::Level;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let clargs = JobArgs::new();
    let config: Config = Config::init(None)?;
    let log_path = config
        .slurmlet
        .log_dir
        .clone()
        .unwrap_or_else(|| "./logs".to_string());
    let path: &Path = Path::new(&log_path);
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }

    let mut filter = EnvFilter::from_default_env();
    if Some("debug".to_string()) == config.slurmlet.log_level {
        filter = filter.add_directive(Level::ERROR.into());
        filter = filter.add_directive(Level::WARN.into());
        filter = filter.add_directive(Level::INFO.into());
        filter = filter.add_directive(Level::DEBUG.into());
    } else if Some("info".to_string()) == config.slurmlet.log_level {
        filter = filter.add_directive(Level::ERROR.into());
        filter = filter.add_directive(Level::WARN.into());
        filter = filter.add_directive(Level::INFO.into());
    } else if Some("warn".to_string()) == config.slurmlet.log_level {
        filter = filter.add_directive(Level::ERROR.into());
        filter = filter.add_directive(Level::WARN.into());
    } else if Some("error".to_string()) == config.slurmlet.log_level {
        filter = filter.add_directive(Level::ERROR.into());
    } else {
        filter = filter.add_directive(Level::ERROR.into());
        filter = filter.add_directive(Level::WARN.into());
        filter = filter.add_directive(Level::INFO.into());
    }

    let appender = tracing_appender::rolling::daily(&log_path, "slurmlet.log");
    let (non_blocking_appender, _guard) = tracing_appender::non_blocking(appender);
    let subscriber = fmt::Subscriber::builder()
        .with_writer(non_blocking_appender)
        .with_env_filter(filter)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set tracing default subscriber");

    if let Some(commands) = clargs.commands {
        let res = handle_exec(commands).await;
    }
    return Ok(());
}
