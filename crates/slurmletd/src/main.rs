use std::{error::Error, fs::File, path::Path};

use common::{arg::DaemonArgs, config::Config};
use daemonize::Daemonize;
use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn Error>> {
    let clargs = DaemonArgs::new();

    let load = clargs.load;
    let config: Config = Config::init(clargs.config.clone())?;

    let log_path = config
        .slurmlet
        .log_dir
        .clone()
        .unwrap_or_else(|| "./logs".to_string());
    let path: &Path = Path::new(&log_path);
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }

    let log_level = config.slurmlet.log_level.as_deref().unwrap_or("info");
    let filter = match log_level {
        "debug" => EnvFilter::new("debug"),
        "info" => EnvFilter::new("info"),
        "warn" => EnvFilter::new("warn"),
        "error" => EnvFilter::new("error"),
        _ => EnvFilter::new("info"),
    };

    let appender = tracing_appender::rolling::daily(&log_path, "slurmlet.log");
    let (non_blocking_appender, _guard) = tracing_appender::non_blocking(appender);
    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_writer(non_blocking_appender)
        .with_env_filter(filter)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set tracing default subscriber");

    // ====================================================

    let daemonize = Daemonize::new()
        .pid_file(
            config
                .slurmlet
                .pid
                .clone()
                .unwrap_or_else(|| "/tmp/slurmlet.pid".to_string()),
        )
        .stdout(File::create(
            config
                .slurmlet
                .stdout
                .clone()
                .unwrap_or_else(|| "./logs/stdout.log".to_string()),
        )?)
        .stderr(File::create(
            config
                .slurmlet
                .stderr
                .clone()
                .unwrap_or_else(|| "./logs/stderr.log".to_string()),
        )?)
        .privileged_action(|| "Executed before drop privileges");

    match daemonize.start() {
        Ok(_) => {
            if load {
                // load tasks
            }
            // start server
            println!("Daemon started");
        }
        Err(e) => eprintln!("Error, {}", e),
    }

    Ok(())
}
