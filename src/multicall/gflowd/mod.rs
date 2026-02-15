use clap::Parser;
use std::ffi::OsString;

mod cli;
mod commands;
mod events;
mod executor;
mod scheduler_runtime;
mod server;
mod state_saver;
mod webhooks;

pub async fn run(argv: Vec<OsString>) -> anyhow::Result<()> {
    let gflowd = cli::GFlowd::parse_from(argv);

    // Initialize tracing: console (stderr) + daily rolling file appender
    let log_dir = gflow::core::get_data_dir()?.join("logs");
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

    let console_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from(
            gflowd.verbosity,
        ))
        .with(console_layer)
        .with(file_layer)
        .init();

    if let Some(command) = gflowd.command {
        return commands::handle_commands(&gflowd.config, command).await;
    }

    let mut config = gflow::config::load_config(gflowd.config.as_ref())?;

    // CLI flag overrides config file
    if let Some(ref gpu_spec) = gflowd.gpus_internal {
        let indices = gflow::utils::parse_gpu_indices(gpu_spec)?;
        config.daemon.gpus = Some(indices);
    }

    server::run(config).await
}
