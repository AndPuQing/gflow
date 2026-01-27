use axum::{http::StatusCode, response::IntoResponse};
use gflow::metrics;

// Metrics endpoint
#[axum::debug_handler]
pub(in crate::server) async fn get_metrics() -> impl IntoResponse {
    match metrics::export_metrics() {
        Ok(text) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4")],
            text,
        ),
        Err(e) => {
            tracing::error!("Failed to export metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "text/plain; version=0.0.4")],
                String::from("Error exporting metrics"),
            )
        }
    }
}
