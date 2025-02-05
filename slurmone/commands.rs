use init::handle_init;
use restart::handle_restart;
use rmp_serde;
use start::handle_start;
use std::error::Error;
use stop::handle_stop;
use tokio::{io::AsyncWriteExt, net::TcpStream};
use tracing::error;

use crate::common::{arg::Commands, config::Config};
mod init;
mod restart;
mod start;
mod stop;

pub async fn handle_exec(commands: Commands, config: Config) -> Result<(), Box<dyn Error>> {
    match commands {
        Commands::Start(start_args) => handle_start(start_args, &config).await?,
        Commands::Stop(stop_args) => handle_stop(stop_args, &config).await?,
        Commands::Restart(restart_args) => handle_restart(restart_args, &config).await?,
        Commands::Init(init_args) => handle_init(init_args, &config).await?,
        _ => handle_other_commands(commands, &config).await?,
    }
    Ok(())
}

async fn handle_other_commands(commands: Commands, config: &Config) -> Result<(), Box<dyn Error>> {
    let sock_path = format!("{}:{}", config.http.host, config.http.port);

    match TcpStream::connect(&sock_path).await {
        Ok(mut sock) => {
            // Serialize the command and send it to the server
            match rmp_serde::to_vec(&commands) {
                Ok(command_bytes) => {
                    if let Err(e) = sock.write_all(&command_bytes).await {
                        error!("Failed to send command to server: {}", e);
                        return Err(e.into());
                    }
                }
                Err(e) => {
                    error!("Failed to serialize command: {}", e);
                    return Err(e.into());
                }
            }
        }
        Err(e) => {
            error!("Failed to connect to server at {}: {}", sock_path, e);
            return Err(e.into());
        }
    }

    Ok(())
}
