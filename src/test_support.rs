//! Test-only helpers shared across modules.
//!
//! Some tests redirect gflow's XDG directories so job logs and runner metadata
//! land in a temp dir instead of the developer's real data home. `set_var` is
//! process-wide, so those tests must not interleave with each other even when
//! they live in different modules. They all serialize on [`env_lock`].

use std::sync::{Mutex, MutexGuard, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Serializes tests that mutate process-wide environment variables.
pub fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
