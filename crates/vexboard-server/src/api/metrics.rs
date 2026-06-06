use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::get,
    Json, Router,
};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stream", get(metrics_stream))
        .route("/snapshot", get(metrics_snapshot))
}

/// SSE endpoint streaming live system metrics and service status events.
#[utoipa::path(
    get,
    path = "/api/v1/metrics/stream",
    tag = "metrics",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Server-sent event stream of SystemSnapshot objects (text/event-stream)",
         content_type = "text/event-stream"),
        (status = 401, description = "Not authenticated"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn metrics_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.metrics_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(snapshot) => {
            let data = serde_json::to_string(&snapshot).unwrap_or_default();
            Some(Ok(Event::default().event("system").data(data)))
        }
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

/// Single JSON snapshot of current system metrics (for initial page load).
#[utoipa::path(
    get,
    path = "/api/v1/metrics/snapshot",
    tag = "metrics",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Current system metrics snapshot"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Failed to read system metrics"),
    )
)]
#[tracing::instrument]
pub(crate) async fn metrics_snapshot() -> impl IntoResponse {
    let snapshot = crate::metrics::system::read_snapshot().await;
    match snapshot {
        Ok(s) => (axum::http::StatusCode::OK, Json(json!(s))),
        Err(e) => {
            tracing::error!("Failed to read metrics snapshot: {e}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to read system metrics"})),
            )
        }
    }
}
