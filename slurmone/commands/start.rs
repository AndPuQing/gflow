use std::error::Error;

use tokio::net::TcpListener;
use tracing::{error, info};

use crate::{
    common::{arg::StartArgs, config::Config},
    slurm::Slurm,
};

async fn is_port_available(addr: &str) -> bool {
    match TcpListener::bind(addr).await {
        Ok(_) => true,
        Err(e) => {
            error!("Failed to bind to port {}: {}", addr, e);
            false
        }
    }
}

pub async fn handle_start(args: StartArgs, config: &Config) -> Result<(), Box<dyn Error>> {
    let sock_addr = format!("{}:{}", config.http.host, config.http.port);

    // 检测端口是否可用
    if !is_port_available(&sock_addr).await {
        println!("Port {} is already in use.", sock_addr);
        return Err(format!("Port {} is already in use.", sock_addr).into());
    }

    // Check if running in daemon mode
    if args.daemon {
        todo!("Daemon mode is not implemented yet");
    }

    // Running in foreground mode
    info!("Starting slurmletd in foreground mode on {}", sock_addr);

    // Initialize and start server
    let server = Slurm::new();
    server.start();

    // Start TCP listener
    server.listen_tcp(&sock_addr).await.map_err(|e| {
        error!("Failed to start TCP listener on {}: {}", sock_addr, e);
        e
    })?;
    Ok(())
}
