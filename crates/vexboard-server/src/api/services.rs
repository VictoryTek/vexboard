use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;

use crate::db;
use crate::db::models::{CreateService, Service, ServiceWithStatus, UpdateService};
use crate::AppState;
use tower_sessions::Session;

#[derive(sqlx::FromRow)]
struct LatestProbe {
    service_id: i64,
    status: String,
    latency_ms: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_services).post(create_service))
        .route("/{id}", put(update_service).delete(delete_service))
        .route("/{id}/claim", post(claim_service))
}

#[tracing::instrument(skip(state))]
async fn list_services(State(state): State<AppState>) -> impl IntoResponse {
    let svcs = match sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible, created_at, updated_at \
         FROM services WHERE visible = 1 ORDER BY sort_order ASC",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to list services: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch services"})),
            );
        }
    };

    // Fetch the latest probe result for every service in a single query,
    // then join in memory — avoids an N+1 round-trip per service.
    let probe_map: HashMap<i64, (String, Option<i64>)> = sqlx::query_as::<_, LatestProbe>(
        "SELECT service_id, status, latency_ms \
         FROM probe_results \
         WHERE (service_id, checked_at) IN \
               (SELECT service_id, MAX(checked_at) FROM probe_results GROUP BY service_id)",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|p| (p.service_id, (p.status, p.latency_ms)))
    .collect();

    let result: Vec<ServiceWithStatus> = svcs
        .into_iter()
        .map(|svc| {
            let (status, latency_ms) = probe_map
                .get(&svc.id)
                .cloned()
                .unwrap_or_else(|| ("unknown".to_string(), None));
            ServiceWithStatus {
                service: svc,
                status,
                latency_ms,
            }
        })
        .collect();

    (StatusCode::OK, Json(json!(result)))
}

#[tracing::instrument(skip(state, session))]
async fn create_service(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateService>,
) -> impl IntoResponse {
    let tags_json = payload
        .tags
        .map(|t| serde_json::to_string(&t).unwrap_or_default());

    let result = sqlx::query(
           "INSERT INTO services (systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
            sort_order, probe_enabled, probe_interval, tags, visible) \
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&payload.systemd_unit)
        .bind(&payload.discovery_source)
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
        Ok(r) => {
            let actor = session
                .get::<String>("username")
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());
            let detail = serde_json::json!({"display_name": payload.display_name}).to_string();
            db::audit::insert(
                &state.db,
                &actor,
                "service.create",
                Some("service"),
                Some(r.last_insert_rowid()),
                Some(&detail),
                None,
            )
            .await;
            (
                StatusCode::CREATED,
                Json(json!({"id": r.last_insert_rowid()})),
            )
        }
        Err(e) => {
            tracing::error!("Failed to create service: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create service"})),
            )
        }
    }
}

#[tracing::instrument(skip(state, session))]
async fn update_service(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateService>,
) -> impl IntoResponse {
    // Build dynamic update query
    // For simplicity, do a full update with fetched defaults
    let existing = sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
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
    let discovery_source = payload.discovery_source.or(existing.discovery_source);
    // Empty string means "clear the field"; None means "keep existing"
    let description = payload
        .description
        .map(|v| if v.is_empty() { None } else { Some(v) })
        .unwrap_or(existing.description);
    let url = payload
        .url
        .map(|v| if v.is_empty() { None } else { Some(v) })
        .unwrap_or(existing.url);
    let icon = payload
        .icon
        .map(|v| if v.is_empty() { None } else { Some(v) })
        .unwrap_or(existing.icon);
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
        "UPDATE services SET discovery_source = ?, display_name = ?, description = ?, url = ?, icon = ?, \
         group_id = ?, sort_order = ?, probe_enabled = ?, probe_interval = ?, \
         tags = ?, visible = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&discovery_source)
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
                "service.update",
                Some("service"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "updated"})))
        }
        Err(e) => {
            tracing::error!("Failed to update service: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update service"})),
            )
        }
    }
}

#[tracing::instrument(skip(state, session))]
async fn delete_service(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM services WHERE id = ?")
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
                "service.delete",
                Some("service"),
                Some(id),
                None,
                None,
            )
            .await;
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
#[tracing::instrument(skip(state, session))]
async fn claim_service(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<CreateService>,
) -> axum::response::Response {
    if let Some(ref unit) = payload.systemd_unit {
        let exists =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM services WHERE systemd_unit = ?")
                .bind(unit)
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);

        if exists > 0 {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "Unit already claimed"})),
            )
                .into_response();
        }
    }

    // Reuse create logic (also writes service.create audit entry)
    create_service(State(state), session, Json(payload))
        .await
        .into_response()
}
