use std::path::Path;

use anyhow::{Context, Result};

pub(crate) static GFLOW_SERVICE: &str = "[Unit]
Description=gflow task scheduler
Documentation=https://example.com/docs/gflow

[Service]
ExecStart={bin} -vvv
ExecStopPost={bin} --cleanup

Restart=on-failure

RuntimeDirectory=gflow
RuntimeDirectoryMode=0755
StateDirectory=gflow
StateDirectoryMode=0700
CacheDirectory=gflow
CacheDirectoryMode=0750

Type=simple

[Install]
WantedBy=multi-user.target";

struct ServiceManager {
    service_file: &'static str,
    binary_path: String,
}

impl ServiceManager {
    fn new() -> Result<Self> {
        let binary_path = shellexpand::tilde("~/.cargo/bin/gflowd").to_string();

        Ok(Self {
            service_file: "/usr/lib/systemd/system/gflowd.service",
            binary_path,
        })
    }

    fn check_binary(&self) -> Result<()> {
        if !Path::new(&self.binary_path).exists() {
            anyhow::bail!("gflowd binary not found at {}", self.binary_path);
        }
        Ok(())
    }

    fn create_service_file(&self) -> Result<()> {
        if Path::new(self.service_file).exists() {
            log::debug!("Service file already exists -> {}", self.service_file);
            return Ok(());
        }

        let service = GFLOW_SERVICE.replace("{bin}", &self.binary_path);
        std::fs::write(self.service_file, service).context("Failed to write service file")?;

        log::debug!("Service file created -> {}", self.service_file);
        Ok(())
    }

    fn start_service(&self) -> Result<()> {
        let output = std::process::Command::new("systemctl")
            .arg("start")
            .arg("gflowd")
            .output()
            .context("Failed to execute systemctl command")?;

        if output.status.success() {
            log::info!("Service started successfully");
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to start service: {}", error)
        }
    }
}

pub(crate) fn handle_start() -> Result<()> {
    let manager = ServiceManager::new()?;

    manager.check_binary().context("Binary check failed")?;

    manager
        .create_service_file()
        .context("Service file creation failed")?;

    manager.start_service().context("Service start failed")?;

    Ok(())
}
