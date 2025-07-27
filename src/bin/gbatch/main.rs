use clap::{CommandFactory, Parser};
use cli::GBatch;
use commands::handle_commands;
mod cli;
mod commands;
mod config;
mod help;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let gflow = GBatch::parse();

    // Initialize logger
    env_logger::Builder::new()
        .filter_level(gflow.verbose.log_level_filter())
        .init();

    log::debug!("Starting gflow with args: {:?}", gflow);

    // Handle commands if present
    let config = config::load_config(&gflow).unwrap_or_else(|err| {
        log::error!("Failed to load config: {}", err);
        std::process::exit(1);
    });

    // Create default config file if it doesn't exist
    if let Ok(config_path) = gflow::core::get_config_dir().map(|d| d.join("gflow.toml")) {
        if !config_path.exists() {
            let default_config = r#"# gflow configuration

# Host and port for the gflowd daemon.
# host = "127.0.0.1"
# port = 59000
"#;
            if let Some(parent) = config_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent).ok();
                }
            }
            std::fs::write(config_path, default_config).ok();
        }
    }

    if let Some(commands) = gflow.commands {
        let output = handle_commands(&config, commands).await;
        if let Err(e) = output {
            log::error!("Error: {}", e);
            std::process::exit(1);
        }
    } else {
        // Show help when no command is provided
        let _ = GBatch::command().print_help();
    }
}
