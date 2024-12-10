use restart::handle_restart;
use rmp_serde;
use start::handle_start;
use std::{error::Error, path::Path};
use stop::handle_stop;
use tokio::{io::AsyncWriteExt, net::UnixStream};

use crate::common::{arg::Commands, config::Config};
mod restart;
mod start;
mod stop;

pub async fn handle_exec(commands: Commands, config: Config) -> Result<(), Box<dyn Error>> {
    match commands {
        Commands::Start(start_args) => handle_start(start_args, &config).await?,
        Commands::Stop(stop_args) => handle_stop(stop_args, &config).await?,
        Commands::Restart(restart_args) => handle_restart(restart_args, &config).await?,
        _ => handle_other_commands(commands, &config).await?,
    }
    Ok(())
}

async fn handle_other_commands(_commands: Commands, config: &Config) -> Result<(), Box<dyn Error>> {
    let sock_path = Path::new(&config.sock.path);
    if !sock_path.exists() {
        return Err("SlurmOned is not running".into());
    }
    let mut sock = UnixStream::connect(sock_path).await?;
    
    // sending the command to the server
    let command_bytes = rmp_serde::to_vec(&_commands)?;
    let _ = sock.write_all(&command_bytes).await?;
    Ok(())
}
