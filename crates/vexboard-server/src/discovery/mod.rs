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

use crate::AppState;

/// A discovered systemd unit not yet claimed by the user.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredUnit {
    pub unit_name: String,
    pub description: String,
    pub active_state: String,
    pub sub_state: String,
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
#[tracing::instrument(skip(state))]
async fn list_discovered(State(state): State<AppState>) -> impl IntoResponse {
    let discoveries = state.discoveries.read().await;
    (StatusCode::OK, Json(json!(*discoveries)))
}

/// Trigger an immediate re-scan of systemd units.
#[tracing::instrument(skip(state))]
async fn trigger_refresh(State(state): State<AppState>) -> impl IntoResponse {
    // Spawn a refresh in the background
    let discoveries = state.discoveries.clone();
    let db = state.db.clone();
    let config = state.config.clone();
    tokio::spawn(async move {
        if let Err(e) = systemd::discover_units(&discoveries, &db, &config.discovery).await {
            tracing::error!("Discovery refresh failed: {e}");
        }
    });
    (StatusCode::ACCEPTED, Json(json!({"status": "refresh triggered"})))
}
