use clap::Parser;
use std::ffi::OsString;

mod cli;
mod commands;
mod emails;
mod events;
mod executor;
mod scheduler_runtime;
mod server;
mod state_saver;
mod webhooks;

pub async fn run(argv: Vec<OsString>) -> anyhow::Result<()> {
    let gflowd = cli::GFlowd::parse_from(argv);

    // Initialize tracing: console (stderr) + daily rolling file appender
    let log_dir = gflow::paths::get_data_dir()?.join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("daemon")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_dir)?;
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true);

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(false)
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(true)
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from(
            gflowd.verbosity,
        ))
        .with(console_layer)
        .with(file_layer)
        .init();

    if let Some(command) = gflowd.command {
        return commands::handle_commands(&gflowd.config, gflowd.verbosity, command).await;
    }

    // Every hosting mode (direct process, tmux, systemd) takes the instance
    // lock for the daemon's whole lifetime. This guarantees mutual exclusion:
    // a second daemon cannot start while one is live, which is what keeps two
    // schedulers from binding the same port (SO_REUSEPORT would otherwise
    // silently load-balance requests across them) and from clobbering each
    // other's state files.
    //
    // The lock file body carries the daemon identity and hosting mode so
    // `down`/`restart`/`status` can refuse to signal a recycled PID and can
    // tear down a tmux-hosted daemon through its session.
    let host_mode = if gflowd.direct_internal {
        commands::lifecycle::DaemonHostMode::Direct
    } else {
        commands::lifecycle::DaemonHostMode::Supervised
    };
    // A replacement daemon launched by `gflowd reload`/`restart` intentionally
    // overlaps with its predecessor's shutdown, so wait briefly for the lock
    // instead of failing instantly. Both the lock and the listening socket are
    // released when the predecessor exits, so acquiring the lock means the port
    // is free to bind exclusively.
    let _instance_lock = match commands::lifecycle::acquire_daemon_lock_blocking(
        std::time::Duration::from_secs(30),
    )? {
        Some(mut file) => {
            let pid = std::process::id() as u32;
            let identity = commands::lifecycle::DaemonIdentity {
                pid,
                pgid: unsafe { libc::getpgid(pid as libc::pid_t) },
                start_time: commands::lifecycle::process_start_time(pid),
                mode: host_mode,
            };
            commands::lifecycle::write_daemon_identity(&mut file, &identity)?;
            tracing::info!(
                pid,
                pgid = identity.pgid,
                mode = ?host_mode,
                "daemon acquired instance flock lock"
            );
            Some(file)
        }
        None => {
            let holder = commands::lifecycle::read_daemon_identity()
                .map(|identity| format!("PID {} ({:?})", identity.pid, identity.mode))
                .unwrap_or_else(|| "an unknown process".to_string());
            anyhow::bail!(
                "another gflowd instance is already running ({holder}); refusing to start \
                 a duplicate. Two daemons would share the daemon port and serve \
                 divergent job queues. Use `gflowd status` or `gflowd down` first."
            );
        }
    };

    let mut config = gflow::config::load_config(gflowd.config.as_ref())?;

    // CLI flag overrides config file
    if let Some(ref gpu_spec) = gflowd.gpus_internal {
        let indices = gflow::utils::parse_gpu_indices(gpu_spec)?;
        config.daemon.gpus = Some(indices);
    }
    if let Some(ref strategy) = gflowd.gpu_allocation_strategy_internal {
        config.daemon.gpu_allocation_strategy = strategy.parse().map_err(|_| {
            anyhow::anyhow!(
                "Invalid GPU allocation strategy '{}'. Use 'sequential' or 'random'.",
                strategy
            )
        })?;
    }
    if let Some(gpu_poll_interval_secs) = gflowd.gpu_poll_interval_secs_internal {
        if gpu_poll_interval_secs == 0 {
            anyhow::bail!("Invalid GPU poll interval '0'. Use a value of at least 1 second.");
        }
        config.daemon.gpu_poll_interval_secs = gpu_poll_interval_secs;
    }

    server::run(config).await
}
