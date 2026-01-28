// Use mimalloc only on x86_64 to avoid cross-compilation issues
// ARM cross-compilers don't support the -Wdate-time flag used by libmimalloc-sys
#[cfg(target_arch = "x86_64")]
use mimalloc::MiMalloc;

#[cfg(target_arch = "x86_64")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod client;
pub mod config;
pub mod core;
pub mod debug;
pub mod metrics;
pub mod tmux;
pub mod utils;

// Re-export commonly used types for convenience
pub use client::Client;
pub use config::Config;

/// Creates a client from the config file path.
/// This is a convenience function to reduce boilerplate in CLI tools.
///
/// # Example
/// ```no_run
/// use gflow::create_client;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config_path: Option<PathBuf> = None;
/// let client = create_client(&config_path)?;
/// # Ok(())
/// # }
/// ```
pub fn create_client(config_path: &Option<std::path::PathBuf>) -> anyhow::Result<Client> {
    let config = config::load_config(config_path.as_ref())?;
    Client::build(&config)
}
