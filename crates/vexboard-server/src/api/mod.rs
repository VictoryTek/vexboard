pub mod auth;
pub mod groups;
pub mod health;
pub mod metrics;
pub mod quick_links;
pub mod services;
pub mod setup;

use crate::AppState;
use axum::Router;

/// Build the complete API router under `/api/v1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/setup/status", axum::routing::get(setup::status))
        .route("/api/v1/setup", axum::routing::post(setup::create_admin))
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/services", services::router())
        .nest("/api/v1/groups", groups::router())
        .nest("/api/v1/quick-links", quick_links::router())
        .nest("/api/v1/metrics", metrics::router())
        .nest("/api/v1/discovery", crate::discovery::router())
        .route("/health", axum::routing::get(health::health_check))
}
