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

    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_max_level(gflowd.verbosity)
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
