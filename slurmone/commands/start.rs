use std::error::Error;

use tracing::info;

use crate::{
    common::{arg::StartArgs, config::Config},
    slurm::Slurm,
    slurmoned::start_daemon,
};

pub async fn handle_start(args: StartArgs, _config: &Config) -> Result<(), Box<dyn Error>> {
    if args.daemon {
        let config: Config = Config::init(None)?;
        let _ = start_daemon(config);
    } else {
        let config: Config = Config::init(None)?;

        let sock_path = config.sock.path.clone();
        info!("Slurmletd started on {}", sock_path);
        // Start the server
        let _server = Slurm::new();
        _server.start();
        _server.listen_unix_socket(&sock_path).await?;
    }
    Ok(())
}
