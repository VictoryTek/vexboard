use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::json;

use crate::db::models::{CreateService, Service, ServiceWithStatus, UpdateService};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_services).post(create_service))
        .route("/:id", put(update_service).delete(delete_service))
        .route("/:id/claim", post(claim_service))
}

#[tracing::instrument(skip(state))]
async fn list_services(State(state): State<AppState>) -> impl IntoResponse {
    let services = sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible, created_at, updated_at \
         FROM services WHERE visible = 1 ORDER BY sort_order ASC",
    )
    .fetch_all(&state.db)
    .await;

    match services {
        Ok(svcs) => {
            let mut result = Vec::with_capacity(svcs.len());
            for svc in svcs {
                // Get latest probe result
                let probe = sqlx::query_as::<_, crate::db::models::ProbeResult>(
                    "SELECT id, service_id, status, latency_ms, checked_at \
                     FROM probe_results WHERE service_id = ? ORDER BY checked_at DESC LIMIT 1",
                )
                .bind(svc.id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

                let (status, latency_ms) = match probe {
                    Some(p) => (p.status, p.latency_ms),
                    None => ("unknown".to_string(), None),
                };

                result.push(ServiceWithStatus {
                    service: svc,
                    status,
                    latency_ms,
                });
            }
            (StatusCode::OK, Json(json!(result)))
        }
        Err(e) => {
            tracing::error!("Failed to list services: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch services"})),
            )
        }
    }
}

#[tracing::instrument(skip(state))]
async fn create_service(
    State(state): State<AppState>,
    Json(payload): Json<CreateService>,
) -> impl IntoResponse {
    let tags_json = payload.tags.map(|t| serde_json::to_string(&t).unwrap_or_default());

    let result = sqlx::query(
        "INSERT INTO services (systemd_unit, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&payload.systemd_unit)
    .bind(&payload.display_name)
    .bind(&payload.description)
    .bind(&payload.url)
    .bind(&payload.icon)
    .bind(payload.group_id)
    .bind(payload.sort_order.unwrap_or(0))
    .bind(payload.probe_enabled.unwrap_or(true))
    .bind(payload.probe_interval.unwrap_or(30))
    .bind(&tags_json)
    .bind(payload.visible.unwrap_or(true))
    .execute(&state.db)
    .await;

    match result {
        Ok(r) => (
            StatusCode::CREATED,
            Json(json!({"id": r.last_insert_rowid()})),
        ),
        Err(e) => {
            tracing::error!("Failed to create service: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create service"})),
            )
        }
    }
}

#[tracing::instrument(skip(state))]
async fn update_service(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateService>,
) -> impl IntoResponse {
    // Build dynamic update query
    let mut sets = Vec::new();
    let mut binds: Vec<Box<dyn std::fmt::Display>> = Vec::new();

    if let Some(ref name) = payload.display_name {
        sets.push("display_name = ?");
        binds.push(Box::new(name.clone()));
    }
    if let Some(ref desc) = payload.description {
        sets.push("description = ?");
        binds.push(Box::new(desc.clone()));
    }
    if let Some(ref url) = payload.url {
        sets.push("url = ?");
        binds.push(Box::new(url.clone()));
    }
    if let Some(ref icon) = payload.icon {
        sets.push("icon = ?");
        binds.push(Box::new(icon.clone()));
    }

    // For simplicity, do a full update with fetched defaults
    let existing = sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible, created_at, updated_at \
         FROM services WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let existing = match existing {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Service not found"})),
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

    let display_name = payload.display_name.unwrap_or(existing.display_name);
    let description = payload.description.or(existing.description);
    let url = payload.url.or(existing.url);
    let icon = payload.icon.or(existing.icon);
    let group_id = payload.group_id.or(existing.group_id);
    let sort_order = payload.sort_order.unwrap_or(existing.sort_order);
    let probe_enabled = payload.probe_enabled.unwrap_or(existing.probe_enabled);
    let probe_interval = payload.probe_interval.unwrap_or(existing.probe_interval);
    let visible = payload.visible.unwrap_or(existing.visible);
    let tags_json = payload
        .tags
        .map(|t| serde_json::to_string(&t).unwrap_or_default())
        .or(existing.tags);

    let result = sqlx::query(
        "UPDATE services SET display_name = ?, description = ?, url = ?, icon = ?, \
         group_id = ?, sort_order = ?, probe_enabled = ?, probe_interval = ?, \
         tags = ?, visible = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&display_name)
    .bind(&description)
    .bind(&url)
    .bind(&icon)
    .bind(group_id)
    .bind(sort_order)
    .bind(probe_enabled)
    .bind(probe_interval)
    .bind(&tags_json)
    .bind(visible)
    .bind(id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "updated"}))),
        Err(e) => {
            tracing::error!("Failed to update service: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update service"})),
            )
        }
    }
}

#[tracing::instrument(skip(state))]
async fn delete_service(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM services WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            (StatusCode::OK, Json(json!({"status": "deleted"})))
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Service not found"})),
        ),
        Err(e) => {
            tracing::error!("Failed to delete service: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to delete service"})),
            )
        }
    }
}

/// Claim a discovered systemd unit — copies it into the services table with user-provided metadata.
#[tracing::instrument(skip(state))]
async fn claim_service(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<CreateService>,
) -> impl IntoResponse {
    // Check if already claimed
    if let Some(ref unit) = payload.systemd_unit {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM services WHERE systemd_unit = ?",
        )
        .bind(unit)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        if exists > 0 {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "Unit already claimed"})),
            );
        }
    }

    // Reuse create logic
    create_service(State(state), Json(payload)).await
}
