use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_sessions::Session;

use crate::db;
use crate::db::models::{
    CreateNotificationChannel, NotificationChannel, UpdateNotificationChannel,
};
use crate::notify;
use crate::probe::uptime::ProbeEvent;
use crate::AppState;

const VALID_KINDS: [&str; 5] = ["webhook", "discord", "ntfy", "telegram", "gotify"];

/// Telegram and Gotify have no unsigned mode — unlike the webhook kind's
/// optional HMAC secret, their `secret` field holds a required credential
/// (bot token / app token).
fn requires_secret(kind: &str) -> bool {
    matches!(kind, "telegram" | "gotify")
}

/// Every route here is admin-only, with no read tier for viewers — unlike
/// services/groups, a channel's `target` can itself function as a bearer
/// credential (anyone with a Discord webhook URL can post to it).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels", get(list_channels).post(create_channel))
        .route(
            "/channels/{id}",
            axum::routing::patch(update_channel).delete(delete_channel),
        )
        .route("/channels/{id}/test", post(test_channel))
        .route("/rules", get(get_rules).patch(update_rules))
}

#[utoipa::path(
    get,
    path = "/api/v1/notifications/channels",
    tag = "notifications",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of notification channels", body = Vec<NotificationChannel>),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_channels(State(state): State<AppState>) -> impl IntoResponse {
    let channels = sqlx::query_as::<_, NotificationChannel>(
        "SELECT id, name, kind, target, secret, events, enabled, created_at \
         FROM notification_channels ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await;

    match channels {
        Ok(c) => (StatusCode::OK, Json(json!(c))),
        Err(e) => {
            tracing::error!("Failed to list notification channels: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch notification channels"})),
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/notifications/channels",
    tag = "notifications",
    security(("cookieAuth" = [])),
    request_body = CreateNotificationChannel,
    responses(
        (status = 201, description = "Channel created; returns new ID"),
        (status = 400, description = "Invalid kind"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn create_channel(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateNotificationChannel>,
) -> impl IntoResponse {
    if !VALID_KINDS.contains(&payload.kind.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("kind must be one of {VALID_KINDS:?}")})),
        );
    }
    if requires_secret(&payload.kind) && payload.secret.as_deref().unwrap_or("").is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": format!("{} channels require a token in the secret field", payload.kind)}),
            ),
        );
    }

    let events_json = serde_json::to_string(&payload.events).unwrap_or_else(|_| "[]".to_string());

    let result = sqlx::query(
        "INSERT INTO notification_channels (name, kind, target, secret, events) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&payload.name)
    .bind(&payload.kind)
    .bind(&payload.target)
    .bind(&payload.secret)
    .bind(&events_json)
    .execute(&state.db)
    .await;

    match result {
        Ok(r) => {
            let actor = session
                .get::<String>("username")
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());
            db::audit::insert(
                &state.db,
                &actor,
                "notification_channel.create",
                Some("notification_channel"),
                Some(r.last_insert_rowid()),
                Some(&json!({"name": payload.name, "kind": payload.kind}).to_string()),
                None,
            )
            .await;
            (
                StatusCode::CREATED,
                Json(json!({"id": r.last_insert_rowid()})),
            )
        }
        Err(e) => {
            tracing::error!("Failed to create notification channel: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create notification channel"})),
            )
        }
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/notifications/channels/{id}",
    tag = "notifications",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Channel ID")),
    request_body = UpdateNotificationChannel,
    responses(
        (status = 200, description = "Channel updated"),
        (status = 400, description = "Invalid kind"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Channel not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn update_channel(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateNotificationChannel>,
) -> impl IntoResponse {
    let existing = sqlx::query_as::<_, NotificationChannel>(
        "SELECT id, name, kind, target, secret, events, enabled, created_at \
         FROM notification_channels WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let existing = match existing {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Channel not found"})),
            )
        }
        Err(e) => {
            tracing::error!("DB error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            );
        }
    };

    let kind = payload.kind.unwrap_or(existing.kind);
    if !VALID_KINDS.contains(&kind.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("kind must be one of {VALID_KINDS:?}")})),
        );
    }

    let name = payload.name.unwrap_or(existing.name);
    let target = payload.target.unwrap_or(existing.target);
    let secret = payload.secret.unwrap_or(existing.secret);
    if requires_secret(&kind) && secret.as_deref().unwrap_or("").is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{kind} channels require a token in the secret field")})),
        );
    }
    let events_json = match payload.events {
        Some(e) => serde_json::to_string(&e).unwrap_or_else(|_| "[]".to_string()),
        None => existing.events,
    };
    let enabled = payload.enabled.unwrap_or(existing.enabled);

    let result = sqlx::query(
        "UPDATE notification_channels SET name = ?, kind = ?, target = ?, secret = ?, events = ?, enabled = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(&kind)
    .bind(&target)
    .bind(&secret)
    .bind(&events_json)
    .bind(enabled)
    .bind(id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            let actor = session
                .get::<String>("username")
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());
            db::audit::insert(
                &state.db,
                &actor,
                "notification_channel.update",
                Some("notification_channel"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "updated"})))
        }
        Err(e) => {
            tracing::error!("Failed to update notification channel: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update notification channel"})),
            )
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/notifications/channels/{id}",
    tag = "notifications",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Channel ID")),
    responses(
        (status = 200, description = "Channel deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Channel not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn delete_channel(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM notification_channels WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            let actor = session
                .get::<String>("username")
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());
            db::audit::insert(
                &state.db,
                &actor,
                "notification_channel.delete",
                Some("notification_channel"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "deleted"})))
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Channel not found"})),
        ),
        Err(e) => {
            tracing::error!("Failed to delete notification channel: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to delete notification channel"})),
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/notifications/channels/{id}/test",
    tag = "notifications",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Channel ID")),
    responses(
        (status = 200, description = "Test notification delivered"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Channel not found"),
        (status = 502, description = "Delivery failed — see error for the real reason"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn test_channel(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let channel = sqlx::query_as::<_, NotificationChannel>(
        "SELECT id, name, kind, target, secret, events, enabled, created_at \
         FROM notification_channels WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let channel = match channel {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Channel not found"})),
            )
        }
        Err(e) => {
            tracing::error!("DB error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            );
        }
    };

    // A single synthetic "down" transition — deliberately the more urgent
    // shape so the admin sees how a real outage alert would look/sound.
    let test_event = ProbeEvent {
        service_id: 0,
        service_name: "VexBoard Test".to_string(),
        url: None,
        status: "down".to_string(),
        latency_ms: None,
    };
    let notification = notify::build_notification(
        &channel,
        &test_event,
        "service.down",
        Some("up"),
        &state.config.notifications,
    );

    match notify::send_once(&state.probe_client, &notification).await {
        Ok(()) => (StatusCode::OK, Json(json!({"status": "ok"}))),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))),
    }
}

/// How many consecutive failed probes before an outage alerts, and how often
/// (if at all) to repeat the alert while still down. Backed by the generic
/// `settings` key/value table — not `config.toml` — so they're editable here
/// without a restart. Defaults (threshold 1, interval 0) reproduce the
/// original fire-on-every-transition behavior exactly.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertRules {
    pub fail_threshold: i64,
    pub repeat_interval_mins: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/notifications/rules",
    tag = "notifications",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Current alert rules", body = AlertRules),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_rules(State(state): State<AppState>) -> impl IntoResponse {
    let fail_threshold = db::get_setting(&state.db, "notify_fail_threshold")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let repeat_interval_mins = db::get_setting(&state.db, "notify_repeat_interval_mins")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    (
        StatusCode::OK,
        Json(AlertRules {
            fail_threshold,
            repeat_interval_mins,
        }),
    )
}

#[utoipa::path(
    patch,
    path = "/api/v1/notifications/rules",
    tag = "notifications",
    security(("cookieAuth" = [])),
    request_body = AlertRules,
    responses(
        (status = 200, description = "Alert rules updated", body = AlertRules),
        (status = 400, description = "fail_threshold must be >= 1, repeat_interval_mins must be >= 0"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn update_rules(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<AlertRules>,
) -> impl IntoResponse {
    if payload.fail_threshold < 1 || payload.repeat_interval_mins < 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "fail_threshold must be >= 1 and repeat_interval_mins must be >= 0"
            })),
        )
            .into_response();
    }

    let _ = db::set_setting(
        &state.db,
        "notify_fail_threshold",
        &payload.fail_threshold.to_string(),
    )
    .await;
    let _ = db::set_setting(
        &state.db,
        "notify_repeat_interval_mins",
        &payload.repeat_interval_mins.to_string(),
    )
    .await;

    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    db::audit::insert(
        &state.db,
        &actor,
        "notifications.rules.update",
        Some("settings"),
        None,
        Some(&json!(payload).to_string()),
        None,
    )
    .await;

    (StatusCode::OK, Json(payload)).into_response()
}
