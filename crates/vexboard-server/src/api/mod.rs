pub mod audit;
pub mod auth;
pub mod groups;
pub mod health;
pub mod metrics;
pub mod quick_links;
pub mod services;
pub mod setup;

use crate::AppState;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::{Json, Router};
use serde_json::json;
use tower_sessions::Session;

/// Middleware that rejects unauthenticated requests with 401.
async fn require_auth(session: Session, request: Request, next: Next) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(_)) => next.run(request).await.into_response(),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Not authenticated"})),
        )
            .into_response(),
    }
}

/// Build the complete API router under `/api/v1`.
pub fn router() -> Router<AppState> {
    // Routes that require an active session.
    let protected = Router::new()
        .nest("/api/v1/services", services::router())
        .nest("/api/v1/groups", groups::router())
        .nest("/api/v1/quick-links", quick_links::router())
        .nest("/api/v1/metrics", metrics::router())
        .nest("/api/v1/discovery", crate::discovery::router())
        .nest("/api/v1/audit", audit::router())
        .route_layer(middleware::from_fn(require_auth));

    // Public routes: setup bootstrap, auth, and health check.
    Router::new()
        .route("/api/v1/setup/status", axum::routing::get(setup::status))
        .route("/api/v1/setup", axum::routing::post(setup::create_admin))
        .nest("/api/v1/auth", auth::router())
        .route("/health", axum::routing::get(health::health_check))
        .merge(protected)
}
