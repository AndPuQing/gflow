//! Asynchronous background state persistence
//!
//! This module provides a background task that handles saving scheduler state
//! to disk without blocking the main scheduler loop. It uses channel-based
//! communication and time-based batching to minimize I/O overhead while
//! ensuring state is persisted regularly and on graceful shutdown.

use crate::scheduler_runtime::SharedState;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

/// Messages sent to the state saver task
#[derive(Debug, Clone, Copy)]
pub enum StateSaverMessage {
    /// Request to save state (sent when dirty flag is set)
    /// Multiple requests within the save interval are batched
    SaveRequest,

    /// Graceful shutdown - save state immediately and exit task
    Shutdown,
}

/// Handle for communicating with the state saver task
///
/// This handle can be cloned and shared across the application to notify
/// the state saver about state changes and coordinate shutdown.
#[derive(Clone)]
pub struct StateSaverHandle {
    tx: mpsc::UnboundedSender<StateSaverMessage>,
    task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl StateSaverHandle {
    /// Create a new handle with the given channel sender
    pub fn new(tx: mpsc::UnboundedSender<StateSaverMessage>) -> Self {
        Self {
            tx,
            task_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Notify the state saver that state has been modified
    ///
    /// This is a fire-and-forget operation. If the channel is closed or full,
    /// the error is silently ignored since state will be saved eventually
    /// through the periodic interval or on shutdown.
    pub fn mark_dirty(&self) {
        let _ = self.tx.send(StateSaverMessage::SaveRequest);
    }

    /// Request graceful shutdown and wait for the state saver task to complete
    ///
    /// This sends a Shutdown message to the state saver, which will perform
    /// a final save and exit. This method waits for the task to complete.
    pub async fn shutdown_and_wait(&self) -> anyhow::Result<()> {
        // Send shutdown message
        self.tx
            .send(StateSaverMessage::Shutdown)
            .map_err(|e| anyhow::anyhow!("Failed to send shutdown message: {}", e))?;

        // Wait for task to complete
        if let Some(handle) = self.task_handle.lock().await.take() {
            handle
                .await
                .map_err(|e| anyhow::anyhow!("State saver task panicked: {}", e))?;
        }

        Ok(())
    }

    /// Store the task handle for shutdown coordination
    ///
    /// This should be called after spawning the state saver task to enable
    /// waiting for task completion during shutdown.
    pub fn set_task_handle(&self, handle: JoinHandle<()>) {
        if let Ok(mut guard) = self.task_handle.try_lock() {
            *guard = Some(handle);
        }
    }
}

/// Background task that handles state persistence
///
/// This task runs in a loop, waiting for either:
/// - A periodic interval to tick (save if state is dirty)
/// - A message from the scheduler (SaveRequest or Shutdown)
///
/// # Arguments
/// * `shared_state` - The shared scheduler state to persist
/// * `rx` - Channel receiver for state saver messages
/// * `save_interval` - How often to save state (if dirty)
pub async fn run(
    shared_state: SharedState,
    mut rx: mpsc::UnboundedReceiver<StateSaverMessage>,
    save_interval: Duration,
) {
    tracing::info!(
        "State saver started with save interval: {}s",
        save_interval.as_secs()
    );

    let mut interval = tokio::time::interval(save_interval);
    // Skip missed ticks to avoid catching up on past intervals
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Flag to track if we need to save on next interval tick
    let mut pending_save = false;

    loop {
        tokio::select! {
            // Periodic tick - save if we have pending changes
            _ = interval.tick() => {
                if pending_save {
                    tracing::debug!("Periodic save triggered (dirty flag was set)");
                    perform_save(&shared_state).await;
                    pending_save = false;
                }
            }

            // Message from scheduler or shutdown coordinator
            msg = rx.recv() => {
                match msg {
                    Some(StateSaverMessage::SaveRequest) => {
                        // Mark that we need to save on next interval tick
                        // This batches multiple save requests within the interval
                        pending_save = true;
                        tracing::trace!("Save request received, will save on next interval");
                    }
                    Some(StateSaverMessage::Shutdown) => {
                        // Shutdown requested - save immediately regardless of dirty state
                        tracing::info!("State saver received shutdown signal");
                        perform_save(&shared_state).await;
                        tracing::info!("Final state save completed, state saver exiting");
                        break;
                    }
                    None => {
                        // Channel closed unexpectedly - save if pending and exit
                        tracing::warn!("State saver channel closed unexpectedly");
                        if pending_save {
                            tracing::info!("Performing final save before exit");
                            perform_save(&shared_state).await;
                        }
                        break;
                    }
                }
            }
        }
    }

    tracing::info!("State saver task finished");
}

/// Perform a state save operation
///
/// This acquires a write lock on the scheduler state and calls
/// save_state_if_dirty(), which only saves if the dirty flag is set.
async fn perform_save(shared_state: &SharedState) {
    let start = std::time::Instant::now();

    let mut state = shared_state.write().await;
    state.save_state_if_dirty().await;

    let elapsed = start.elapsed();
    if elapsed.as_millis() > 100 {
        tracing::warn!("State save took {}ms (slow I/O)", elapsed.as_millis());
    } else {
        tracing::trace!("State save completed in {}ms", elapsed.as_millis());
    }
}
