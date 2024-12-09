use std::error::Error;

use slurmone::slurm;
use tracing::info;

use crate::{
    common::{arg::StartArgs, config::Config},
    slurmoned::start_daemon,
};

pub async fn start(args: StartArgs) -> Result<(), Box<dyn Error>> {
    if args.daemon {
        let config: Config = Config::init(None)?;
        let _ = start_daemon(config);
    } else {
        let config: Config = Config::init(None)?;

        // Start a PID sock for listening
        let _pid_path = config.slurmone.pid.clone();
        let sock_path = config.sock.path.clone();
        let _ = std::fs::remove_file(&sock_path);
        let _listener = tokio::net::UnixListener::bind(&sock_path)?;

        info!("Slurmletd started on {}", sock_path);

        // Start the server
        let _server = slurm::Slurm::new();
        // let server = server.listen(listener);
    }

    Ok(())
}
