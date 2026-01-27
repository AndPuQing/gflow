use crate::events::EventBus;
use crate::scheduler_runtime::SharedState;
use crate::state_saver::StateSaverHandle;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

/// Server state that includes both the scheduler and the event bus
#[derive(Clone)]
pub(super) struct ServerState {
    pub(super) scheduler: SharedState,
    pub(super) event_bus: Arc<EventBus>,
    pub(super) _state_saver: StateSaverHandle,
}

impl ServerState {
    pub(super) fn new(
        scheduler: SharedState,
        event_bus: Arc<EventBus>,
        state_saver: StateSaverHandle,
    ) -> Self {
        Self {
            scheduler,
            event_bus,
            _state_saver: state_saver,
        }
    }
}

pub(super) async fn reject_if_read_only(server_state: &ServerState) -> Option<Response> {
    let state = server_state.scheduler.read().await;
    if state.can_mutate() {
        return None;
    }

    let backup_path = state.state_backup_path().map(|p| p.display().to_string());
    let journal_path = state.journal_path().display().to_string();

    Some(
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "gflowd is in read-only mode (no persistence available)",
                "detail": state.state_load_error(),
                "state_backup": backup_path,
                "journal": journal_path,
                "journal_error": state.journal_error(),
                "hint": "Fix/upgrade the version that can migrate your state.json, or restore from the backup file. If the journal path is unwritable, fix permissions."
            })),
        )
            .into_response(),
    )
}
