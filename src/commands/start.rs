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

pub(crate) fn handle_start() {
    // Step 1: check if the service file exists
    let service_file = "/usr/lib/systemd/system/gflowd.service";

    let gflowd_binary = "~/.cargo/bin/gflowd";
    // because sudo gflow up will run as root
    // we need to expand the path not as the current user
    let gflowd_binary = shellexpand::tilde(&gflowd_binary).to_string();
    log::debug!("gflowd binary -> {}", gflowd_binary);
    if std::path::Path::new(service_file).exists() {
        log::debug!("Service file already exists -> {}", service_file);
    } else {
        let service = GFLOW_SERVICE.replace("{bin}", &gflowd_binary);
        std::fs::write(service_file, service).expect("Unable to write file");
        log::debug!("Service file created -> {}", service_file);
    }

    // Step 2: check the gflowd binary exists
    if !std::path::Path::new(&gflowd_binary).exists() {
        log::error!("gflowd binary not found -> {}", gflowd_binary);
        std::process::exit(1);
    }

    // Step 3: start the service
    let output = std::process::Command::new("systemctl")
        .arg("start")
        .arg("gflowd")
        .output()
        .expect("Failed to start the service");

    if output.status.success() {
        log::info!("Service started successfully");
    } else {
        log::error!("Failed to start the service");
        log::error!("{}", String::from_utf8_lossy(&output.stderr));
    }
}
