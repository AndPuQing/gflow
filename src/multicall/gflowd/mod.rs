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

    server::run(config).await
}
