use std::{error::Error, fs::File};

use common::config::Config;
use daemonize::Daemonize;
use tracing::info;

pub mod slurm;

pub fn start_daemon(config: Config) -> Result<(), Box<dyn Error>> {
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
            // start server
            info!("Starting slurmletd daemon");
        }
        Err(e) => eprintln!("Error, {}", e),
    }

    Ok(())
}
