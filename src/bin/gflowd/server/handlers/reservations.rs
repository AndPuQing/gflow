use super::super::state::ServerState;
use crate::events::SchedulerEvent;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

use gflow::core::reservation::{GpuReservation, GpuSpec, ReservationStatus};

#[derive(Debug, Deserialize)]
pub struct CreateReservationRequest {
    pub user: String,
    /// GPU count (for count-based reservations)
    pub gpu_count: Option<u32>,
    /// GPU indices (for index-based reservations, e.g., [0, 2, 3])
    pub gpu_indices: Option<Vec<u32>>,
    pub start_time: SystemTime,
    pub duration_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct CreateReservationResponse {
    pub reservation_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct ListReservationsQuery {
    pub user: Option<String>,
    pub status: Option<String>,
    pub active_only: Option<bool>,
}

pub async fn create_reservation(
    State(server_state): State<ServerState>,
    Json(req): Json<CreateReservationRequest>,
) -> Result<Json<CreateReservationResponse>, (StatusCode, String)> {
    let mut state = server_state.scheduler.write().await;

    let duration = Duration::from_secs(req.duration_secs);
    let user = CompactString::from(req.user);

    // Validate that exactly one of gpu_count or gpu_indices is provided
    let gpu_spec = match (req.gpu_count, req.gpu_indices) {
        (Some(count), None) => GpuSpec::Count(count),
        (None, Some(indices)) => {
            if indices.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "gpu_indices cannot be empty".to_string(),
                ));
            }
            GpuSpec::Indices(indices)
        }
        (Some(_), Some(_)) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Cannot specify both gpu_count and gpu_indices".to_string(),
            ));
        }
        (None, None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Must specify either gpu_count or gpu_indices".to_string(),
            ));
        }
    };

    let reservation_id = state
        .create_reservation(user, gpu_spec, req.start_time, duration)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Publish event
    server_state
        .event_bus
        .publish(SchedulerEvent::ReservationCreated { reservation_id });

    Ok(Json(CreateReservationResponse { reservation_id }))
}

pub async fn list_reservations(
    State(server_state): State<ServerState>,
    Query(query): Query<ListReservationsQuery>,
) -> Result<Json<Vec<GpuReservation>>, (StatusCode, String)> {
    let state = server_state.scheduler.read().await;

    // Parse status filter
    let status_filter = if let Some(status_str) = query.status {
        match status_str.to_lowercase().as_str() {
            "pending" => Some(ReservationStatus::Pending),
            "active" => Some(ReservationStatus::Active),
            "completed" => Some(ReservationStatus::Completed),
            "cancelled" => Some(ReservationStatus::Cancelled),
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Invalid status: {}", status_str),
                ))
            }
        }
    } else {
        None
    };

    let reservations = state.list_reservations(
        query.user.as_deref(),
        status_filter,
        query.active_only.unwrap_or(false),
    );

    // Clone reservations to return
    let reservations: Vec<GpuReservation> = reservations.into_iter().cloned().collect();

    Ok(Json(reservations))
}

pub async fn get_reservation(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Result<Json<GpuReservation>, (StatusCode, String)> {
    let state = server_state.scheduler.read().await;

    let reservation = state.get_reservation(id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Reservation {} not found", id),
        )
    })?;

    Ok(Json(reservation.clone()))
}

pub async fn cancel_reservation(
    State(server_state): State<ServerState>,
    Path(id): Path<u32>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut state = server_state.scheduler.write().await;

    state
        .cancel_reservation(id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Publish event
    server_state
        .event_bus
        .publish(SchedulerEvent::ReservationCancelled { reservation_id: id });

    Ok(StatusCode::OK)
}
