use std::{error::Error, fs::File};

use daemonize::Daemonize;
use tracing::info;

use crate::common::config::Config;

pub fn start_daemon(config: Config) -> Result<(), Box<dyn Error>> {
    let daemonize = Daemonize::new()
        .pid_file(
            config
                .slurmone
                .pid
                .clone()
                .unwrap_or_else(|| "/tmp/slurmone.pid".to_string()),
        )
        .stdout(File::create(
            config
                .slurmone
                .stdout
                .clone()
                .unwrap_or_else(|| "./logs/stdout.log".to_string()),
        )?)
        .stderr(File::create(
            config
                .slurmone
                .stderr
                .clone()
                .unwrap_or_else(|| "./logs/stderr.log".to_string()),
        )?)
        .privileged_action(|| "Executed before drop privileges");

    match daemonize.start() {
        Ok(_) => {
            // start server
            info!("Starting slurmoned daemon");
        }
        Err(e) => eprintln!("Error, {}", e),
    }

    Ok(())
}
