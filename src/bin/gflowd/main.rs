use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod cli;
mod commands;
mod executor;
mod scheduler_runtime;
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let gflowd = cli::GFlowd::parse();

    // Map verbosity flag to log level
    let filter = match gflowd.verbose {
        0 => "gflow=info,gflowd=info",
        1 => "gflow=debug,gflowd=debug",
        2 => "gflow=trace,gflowd=trace",
        _ => "trace",
    };

    // Initialize tracing with log bridge
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Bridge log crate to tracing for dependencies (after tracing is initialized)
    tracing_log::LogTracer::init().ok();

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
