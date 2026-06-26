// Use mimalloc only on x86_64 to avoid cross-compilation issues
// ARM cross-compilers don't support the -Wdate-time flag used by libmimalloc-sys
#[cfg(target_arch = "x86_64")]
use mimalloc::MiMalloc;

#[cfg(target_arch = "x86_64")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// Allow referring to this crate as `gflow::...` from within the crate itself.
extern crate self as gflow;

pub mod build_info;
pub mod client;
pub mod config;
pub mod core;
pub mod debug;
pub mod metrics;
pub mod multicall;
pub mod paths;
pub mod platform;
pub mod tls;
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

/// Like [`create_client`] but falls back to the default config when the config
/// file is missing or unreadable. Used by daemon control commands (`gflowd
/// status`/`reload`/`up`) that must work even before a config file exists.
pub fn create_client_or_default(
    config_path: &Option<std::path::PathBuf>,
) -> anyhow::Result<Client> {
    let config = config::load_config(config_path.as_ref()).unwrap_or_default();
    Client::build(&config)
}
