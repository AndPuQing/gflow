// Use mimalloc only on x86_64 to avoid cross-compilation issues
#[cfg(all(feature = "mimalloc", target_arch = "x86_64"))]
use mimalloc::MiMalloc;

#[cfg(all(feature = "mimalloc", target_arch = "x86_64"))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod client;
pub mod config;
pub mod core;
pub mod debug;
pub mod metrics;
pub mod tmux;
pub mod utils;
