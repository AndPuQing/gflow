use super::super::state::{reject_if_read_only, ServerState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use gflow::config::QuotaLimits;
use gflow::core::quota::{QuotaScope, QuotaStatusEntry};

/// `GET /quotas` — effective limits and current usage for every quota subject.
pub(in crate::multicall::gflowd::server) async fn list_quotas(
    State(server_state): State<ServerState>,
) -> Response {
    let state = server_state.scheduler.read().await;
    let quotas: Vec<QuotaStatusEntry> = state.quota_status();
    let overrides = state.quota_overrides();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "quotas": quotas,
            "overrides": overrides,
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub(in crate::multicall::gflowd::server) struct SetQuotaRequest {
    /// `user` | `project` | `default_user` | `default_project`
    scope: QuotaScope,
    /// Subject name; required for `user` / `project`, ignored for defaults.
    #[serde(default)]
    name: Option<String>,
    /// Only `Some` fields are written; unset fields keep their current value.
    #[serde(default)]
    limits: QuotaLimits,
}

/// `PUT /quotas` — merge a runtime override (persisted in daemon state).
pub(in crate::multicall::gflowd::server) async fn set_quota(
    State(server_state): State<ServerState>,
    Json(request): Json<SetQuotaRequest>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }

    if request.scope.is_named() {
        let valid_name = request
            .name
            .as_ref()
            .is_some_and(|name| !name.trim().is_empty());
        if !valid_name {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("'name' is required for scope '{:?}'", request.scope)
                })),
            )
                .into_response();
        }
    }

    if request.limits.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "at least one limit field must be provided"
            })),
        )
            .into_response();
    }

    let overrides = {
        let mut state = server_state.scheduler.write().await;
        state.merge_quota_override(request.scope, request.name.as_deref(), &request.limits);
        state.quota_overrides()
    };

    tracing::info!(
        scope = ?request.scope,
        name = ?request.name,
        "Quota override updated"
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "scope": request.scope,
            "name": request.name,
            "overrides": overrides,
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub(in crate::multicall::gflowd::server) struct DeleteQuotaQuery {
    scope: QuotaScope,
    #[serde(default)]
    name: Option<String>,
}

/// `DELETE /quotas?scope=user&name=alice` — remove a runtime override entry.
pub(in crate::multicall::gflowd::server) async fn delete_quota(
    State(server_state): State<ServerState>,
    Query(query): Query<DeleteQuotaQuery>,
) -> Response {
    if let Some(resp) = reject_if_read_only(&server_state).await {
        return resp;
    }

    if query.scope.is_named() && query.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("'name' is required for scope '{:?}'", query.scope)
            })),
        )
            .into_response();
    }

    let (removed, overrides) = {
        let mut state = server_state.scheduler.write().await;
        let removed = state.remove_quota_override(query.scope, query.name.as_deref());
        (removed, state.quota_overrides())
    };

    if !removed {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no matching quota override"
            })),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "scope": query.scope,
            "name": query.name,
            "removed": true,
            "overrides": overrides,
        })),
    )
        .into_response()
}
