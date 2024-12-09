use std::error::Error;

use common::{arg::StartArgs, config::Config};
use slurmoned::start_daemon;
use tracing::info;

pub async fn start(args: StartArgs) -> Result<(), Box<dyn Error>> {
    if args.daemon {
        let config: Config = Config::init(None)?;
        let _ = start_daemon(config);
    } else {
        let config: Config = Config::init(None)?;

        // Start a PID sock for listening
        let pid_path = config.slurmone.pid.clone();
        let sock_path = config.sock.path.clone();
        let _ = std::fs::remove_file(&sock_path);
        let listener = tokio::net::UnixListener::bind(&sock_path)?;

        info!("Slurmletd started on {}", sock_path);

        // Start the server
        let server = slurmoned::slurm::Slurm::new();
        // let server = server.listen(listener);
    }

    Ok(())
}
