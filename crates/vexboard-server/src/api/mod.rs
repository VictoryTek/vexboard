pub mod audit;
pub mod auth;
pub mod config;
pub mod groups;
pub mod health;
pub mod metrics;
pub mod notifications;
pub mod openapi;
pub mod quick_links;
pub mod services;
pub mod settings;
pub mod setup;
pub mod users;

use crate::middleware::auth::{require_admin, require_auth};
use crate::AppState;
use axum::middleware;
use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Build the complete API router under `/api/v1`.
///
/// `auth_mode` mirrors `config::AuthConfig::mode`: `"none"` skips the
/// `require_auth`/`require_admin` layers entirely (only safe when the
/// network layer itself restricts access); any other value (normally
/// `"session"`) applies them as usual.
pub fn router(auth_mode: &str, state: AppState) -> Router<AppState> {
    let auth_disabled = auth_mode == "none";

    // Read-only routes: viewer and admin.
    let viewer_protected = Router::new()
        .nest("/api/v1/services", services::read_router())
        .nest("/api/v1/groups", groups::read_router())
        .nest("/api/v1/quick-links", quick_links::read_router())
        .nest("/api/v1/metrics", metrics::router());
    let viewer_protected = if auth_disabled {
        viewer_protected
    } else {
        viewer_protected.route_layer(middleware::from_fn(require_auth))
    };

    // Mutating routes: admin only.
    let admin_protected = Router::new()
        .nest("/api/v1/services", services::admin_router())
        .nest("/api/v1/groups", groups::admin_router())
        .nest("/api/v1/quick-links", quick_links::admin_router())
        .nest("/api/v1/discovery", crate::discovery::router())
        .nest("/api/v1/users", users::router())
        .nest("/api/v1/settings", settings::router())
        .nest("/api/v1/notifications", notifications::router())
        .nest("/api/v1/audit", audit::router());
    let admin_protected = if auth_disabled {
        admin_protected
    } else {
        admin_protected.route_layer(middleware::from_fn_with_state(state, require_admin))
    };

    // Public routes: setup bootstrap, auth, health check, public config, and OpenAPI docs.
    Router::new()
        .route("/api/v1/setup/status", axum::routing::get(setup::status))
        .route("/api/v1/setup", axum::routing::post(setup::create_admin))
        .nest("/api/v1/auth", auth::router())
        .route("/health", axum::routing::get(health::health_check))
        .route(
            "/api/v1/config/public",
            axum::routing::get(config::public_config),
        )
        .merge(
            SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi::ApiDoc::openapi()),
        )
        .merge(viewer_protected)
        .merge(admin_protected)
}
