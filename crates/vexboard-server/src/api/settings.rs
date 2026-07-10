use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_sessions::Session;

use crate::db;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/auth-mode", get(get_auth_mode).patch(set_auth_mode))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuthModeStatus {
    /// The auth mode the running process was actually built with at startup.
    pub active_mode: String,
    /// The auth mode stored in the database, applied on the next restart.
    /// Equal to `active_mode` when no override is pending.
    pub stored_mode: String,
    /// True when `stored_mode` differs from `active_mode`, meaning a restart
    /// is needed for the stored value to take effect.
    pub restart_required: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetAuthModeRequest {
    /// Either "session" (login required) or "none" (network-gated, no login).
    pub mode: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/settings/auth-mode",
    tag = "settings",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Current and pending auth mode", body = AuthModeStatus),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_auth_mode(State(state): State<AppState>) -> impl IntoResponse {
    let active_mode = state.config.auth.mode.clone();
    let stored_mode = match db::get_setting(&state.db, "auth_mode").await {
        Ok(Some(v)) if v == "session" || v == "none" => v,
        _ => active_mode.clone(),
    };
    let restart_required = stored_mode != active_mode;
    (
        StatusCode::OK,
        Json(AuthModeStatus {
            active_mode,
            stored_mode,
            restart_required,
        }),
    )
}

#[utoipa::path(
    patch,
    path = "/api/v1/settings/auth-mode",
    tag = "settings",
    security(("cookieAuth" = [])),
    request_body = SetAuthModeRequest,
    responses(
        (status = 200, description = "Auth mode stored", body = AuthModeStatus),
        (status = 400, description = "Invalid mode value"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn set_auth_mode(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<SetAuthModeRequest>,
) -> impl IntoResponse {
    if payload.mode != "session" && payload.mode != "none" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "mode must be 'session' or 'none'"})),
        )
            .into_response();
    }

    if let Err(e) = db::set_setting(&state.db, "auth_mode", &payload.mode).await {
        tracing::error!("Failed to store auth_mode setting: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to store setting"})),
        )
            .into_response();
    }

    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    db::audit::insert(
        &state.db,
        &actor,
        "settings.auth_mode.update",
        Some("settings"),
        None,
        Some(&json!({"mode": payload.mode}).to_string()),
        None,
    )
    .await;

    let active_mode = state.config.auth.mode.clone();
    let restart_required = payload.mode != active_mode;
    (
        StatusCode::OK,
        Json(AuthModeStatus {
            active_mode,
            stored_mode: payload.mode,
            restart_required,
        }),
    )
        .into_response()
}
