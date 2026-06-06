pub mod audit;
pub mod auth;
pub mod groups;
pub mod health;
pub mod metrics;
pub mod openapi;
pub mod quick_links;
pub mod services;
pub mod setup;
pub mod users;

use crate::AppState;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::{Json, Router};
use serde_json::json;
use tower_sessions::Session;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

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

/// Middleware that requires an active session with the `admin` role.
/// Returns 401 if not authenticated, 403 if authenticated but not admin.
async fn require_admin(session: Session, request: Request, next: Next) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(_)) => {}
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
                .into_response()
        }
    }
    match session.get::<String>("role").await {
        Ok(Some(ref r)) if r == "admin" => next.run(request).await.into_response(),
        _ => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Admin role required"})),
        )
            .into_response(),
    }
}

/// Build the complete API router under `/api/v1`.
pub fn router() -> Router<AppState> {
    // Read-only routes: viewer and admin.
    let viewer_protected = Router::new()
        .nest("/api/v1/services", services::read_router())
        .nest("/api/v1/groups", groups::read_router())
        .nest("/api/v1/quick-links", quick_links::read_router())
        .nest("/api/v1/metrics", metrics::router())
        .nest("/api/v1/audit", audit::router())
        .route_layer(middleware::from_fn(require_auth));

    // Mutating routes: admin only.
    let admin_protected = Router::new()
        .nest("/api/v1/services", services::admin_router())
        .nest("/api/v1/groups", groups::admin_router())
        .nest("/api/v1/quick-links", quick_links::admin_router())
        .nest("/api/v1/discovery", crate::discovery::router())
        .nest("/api/v1/users", users::router())
        .route_layer(middleware::from_fn(require_admin));

    // Public routes: setup bootstrap, auth, health check, and OpenAPI docs.
    Router::new()
        .route("/api/v1/setup/status", axum::routing::get(setup::status))
        .route("/api/v1/setup", axum::routing::post(setup::create_admin))
        .nest("/api/v1/auth", auth::router())
        .route("/health", axum::routing::get(health::health_check))
        .merge(
            SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi::ApiDoc::openapi()),
        )
        .merge(viewer_protected)
        .merge(admin_protected)
}
