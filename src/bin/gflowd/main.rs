use clap::Parser;
mod cli;
mod commands;
mod executor;
mod scheduler_runtime;
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let gflowd = cli::GFlowd::parse();
    env_logger::Builder::new()
        .filter_level(gflowd.verbose.log_level_filter())
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
