use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod client;
pub mod config;
pub mod core;
pub mod debug;
pub mod metrics;
pub mod tmux;
pub mod utils;
