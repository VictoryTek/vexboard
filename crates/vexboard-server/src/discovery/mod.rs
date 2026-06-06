pub mod docker;
pub mod systemd;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db;
use crate::AppState;
use tower_sessions::Session;

/// A discovered unit (systemd service or container) not yet claimed by the user.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DiscoveredUnit {
    pub unit_name: String,
    pub description: String,
    pub active_state: String,
    pub sub_state: String,
    /// Origin: "systemd", "docker", or "podman"
    pub source: String,
    /// Suggested URL if detectable (e.g. from exposed container ports)
    pub url_hint: Option<String>,
}

pub type DiscoveryList = Arc<RwLock<Vec<DiscoveredUnit>>>;

pub fn new_discovery_list() -> DiscoveryList {
    Arc::new(RwLock::new(Vec::new()))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_discovered))
        .route("/refresh", post(trigger_refresh))
}

/// List all unclaimed discovered systemd units.
#[utoipa::path(
    get,
    path = "/api/v1/discovery",
    tag = "discovery",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of discovered unclaimed units", body = Vec<DiscoveredUnit>),
        (status = 401, description = "Not authenticated"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_discovered(State(state): State<AppState>) -> impl IntoResponse {
    let discoveries = state.discoveries.read().await;
    (StatusCode::OK, Json(json!(*discoveries)))
}

/// Trigger an immediate re-scan of systemd units.
#[utoipa::path(
    post,
    path = "/api/v1/discovery/refresh",
    tag = "discovery",
    security(("cookieAuth" = [])),
    responses(
        (status = 202, description = "Refresh triggered in background"),
        (status = 401, description = "Not authenticated"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn trigger_refresh(
    State(state): State<AppState>,
    session: Session,
) -> impl IntoResponse {
    // Spawn a systemd refresh in the background
    let discoveries = state.discoveries.clone();
    let db = state.db.clone();
    let config = state.config.clone();
    tokio::spawn(async move {
        if let Err(e) = systemd::discover_units(&discoveries, &db, &config.discovery).await {
            tracing::error!("systemd discovery refresh failed: {e}");
        }
    });
    // Spawn a container refresh in the background
    let discoveries2 = state.discoveries.clone();
    let db2 = state.db.clone();
    let config2 = state.config.clone();
    tokio::spawn(async move {
        if let Err(e) = docker::discover_containers(&discoveries2, &db2, &config2.docker).await {
            tracing::error!("container discovery refresh failed: {e}");
        }
    });
    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    db::audit::insert(
        &state.db,
        &actor,
        "discovery.refresh",
        None,
        None,
        None,
        None,
    )
    .await;
    (
        StatusCode::ACCEPTED,
        Json(json!({"status": "refresh triggered"})),
    )
}
