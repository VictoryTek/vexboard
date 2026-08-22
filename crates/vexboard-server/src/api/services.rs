use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, patch, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::control::{self, UnitAction};
use crate::db;
use crate::db::models::{
    CreateService, ProbeHistoryPoint, ReorderItem, Service, ServiceWithStatus, UpdateService,
};
use crate::probe;
use crate::AppState;
use tower_sessions::Session;

/// A server-sent event stream of raw log lines, regardless of which
/// backend (systemd journal or Docker/Podman) produced it.
type BoxedLogStream =
    std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>>;

fn to_sse_log_stream(
    lines: impl tokio_stream::Stream<Item = std::io::Result<String>> + Send + 'static,
) -> BoxedLogStream {
    use tokio_stream::StreamExt;
    Box::pin(lines.filter_map(|line| line.ok().map(|l| Ok(Event::default().data(l)))))
}

#[derive(sqlx::FromRow)]
struct LatestProbe {
    service_id: i64,
    status: String,
    latency_ms: Option<i64>,
}

/// Safety cap on rows fetched for one service's uptime summary, independent
/// of `probe.history_retention_days` — bounds worst-case query/scan cost if
/// a very short probe interval outpaces the age-based prune cycle.
const MAX_SUMMARY_ROWS: i64 = 20_000;

pub fn read_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_services))
        .route("/stream", get(stream_service_events))
        .route("/{id}/history", get(service_history))
        .route("/{id}/uptime", get(service_uptime_summary))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(crate) struct HistoryQuery {
    /// Maximum number of probe results to return (1-100, default 100).
    #[serde(default = "default_history_limit")]
    limit: i64,
}

fn default_history_limit() -> i64 {
    100
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/reorder", patch(reorder_services))
        .route("/", post(create_service))
        .route("/{id}", put(update_service).delete(delete_service))
        .route("/{id}/claim", post(claim_service))
        .route("/{id}/start", post(start_service))
        .route("/{id}/stop", post(stop_service))
        .route("/{id}/restart", post(restart_service))
        .route("/{id}/logs/stream", get(service_logs_stream))
}

#[utoipa::path(
    get,
    path = "/api/v1/services",
    tag = "services",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of visible services with probe status", body = Vec<ServiceWithStatus>),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_services(State(state): State<AppState>) -> impl IntoResponse {
    let svcs = match sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible, skip_tls_verify, created_at, updated_at \
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

/// SSE endpoint streaming live probe results as they complete.
#[utoipa::path(
    get,
    path = "/api/v1/services/stream",
    tag = "services",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Server-sent event stream of ProbeEvent objects (text/event-stream)",
         content_type = "text/event-stream"),
        (status = 401, description = "Not authenticated"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn stream_service_events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.probe_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(event) => {
            let data = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(Event::default().event("probe").data(data)))
        }
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

/// Recent probe history for a single service, oldest-first.
#[utoipa::path(
    get,
    path = "/api/v1/services/{id}/history",
    tag = "services",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Service ID"),
        HistoryQuery,
    ),
    responses(
        (status = 200, description = "Probe history, oldest-first", body = Vec<ProbeHistoryPoint>),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn service_history(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<HistoryQuery>,
) -> impl IntoResponse {
    let limit = params.limit.clamp(1, 100);

    match sqlx::query_as::<_, ProbeHistoryPoint>(
        "SELECT status, latency_ms, checked_at FROM probe_results \
         WHERE service_id = ? ORDER BY checked_at DESC LIMIT ?",
    )
    .bind(id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    {
        Ok(mut points) => {
            points.reverse();
            (StatusCode::OK, Json(json!(points)))
        }
        Err(e) => {
            tracing::error!("Failed to fetch service history: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch service history"})),
            )
        }
    }
}

/// Uptime percentages (24h/7d/30d), a heartbeat tail, and derived incidents
/// for a single service, built from one fetch of its retained probe history.
#[utoipa::path(
    get,
    path = "/api/v1/services/{id}/uptime",
    tag = "services",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Service ID"),
    ),
    responses(
        (status = 200, description = "Uptime summary", body = crate::db::models::UptimeSummary),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn service_uptime_summary(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match sqlx::query_as::<_, ProbeHistoryPoint>(
        "SELECT status, latency_ms, checked_at FROM probe_results \
         WHERE service_id = ? ORDER BY checked_at DESC LIMIT ?",
    )
    .bind(id)
    .bind(MAX_SUMMARY_ROWS)
    .fetch_all(&state.db)
    .await
    {
        Ok(mut points) => {
            points.reverse();
            let summary =
                probe::uptime::compute_uptime_summary(&points, chrono::Utc::now().naive_utc());
            (StatusCode::OK, Json(json!(summary)))
        }
        Err(e) => {
            tracing::error!("Failed to fetch uptime summary for service {id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch uptime summary"})),
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/services",
    tag = "services",
    security(("cookieAuth" = [])),
    request_body = CreateService,
    responses(
        (status = 201, description = "Service created; returns new ID"),
        (status = 401, description = "Not authenticated"),
        (status = 409, description = "systemd_unit already claimed by another service"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn create_service(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateService>,
) -> impl IntoResponse {
    if let Some(ref unit) = payload.systemd_unit {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM services WHERE systemd_unit = ? LIMIT 1)",
        )
        .bind(unit)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

        if exists {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "Unit already claimed"})),
            );
        }
    }

    let tags_json = match payload.tags {
        Some(t) => match serde_json::to_string(&t) {
            Ok(j) => Some(j),
            Err(e) => {
                tracing::error!("create_service: failed to serialize tags: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Internal error"})),
                );
            }
        },
        None => None,
    };

    let result = sqlx::query(
           "INSERT INTO services (systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
            sort_order, probe_enabled, probe_interval, tags, visible, skip_tls_verify) \
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(payload.skip_tls_verify.unwrap_or(false))
    .execute(&state.db)
    .await;

    match result {
        Ok(r) => {
            let new_id = r.last_insert_rowid();

            if let (Some(source), Some(unit_name)) =
                (&payload.discovery_source, &payload.systemd_unit)
            {
                let mut discoveries = state.discoveries.write().await;
                discoveries.retain(|u| !(&u.source == source && &u.unit_name == unit_name));
                drop(discoveries);
            }

            // Trigger an immediate background probe so status is ready on the
            // next frontend refetch instead of waiting for the next probe cycle.
            let probe_db = state.db.clone();
            let probe_tx = state.probe_tx.clone();
            let probe_client = state.probe_client.clone();
            let probe_client_insecure = state.probe_client_insecure.clone();
            let retention_days = state.config.probe.history_retention_days;
            tokio::spawn(async move {
                if let Ok(Some(svc)) = sqlx::query_as::<_, Service>(
                    "SELECT id, systemd_unit, discovery_source, display_name, description, url, \
                     icon, group_id, sort_order, probe_enabled, probe_interval, tags, visible, \
                     skip_tls_verify, created_at, updated_at FROM services WHERE id = ? AND probe_enabled = 1",
                )
                .bind(new_id)
                .fetch_optional(&probe_db)
                .await
                {
                    // Docker/Podman discoveries store the container name in
                    // `systemd_unit`, not a real systemd unit — only trust it as a
                    // D-Bus lookup key when the service wasn't discovered that way.
                    let use_systemd = svc.systemd_unit.is_some()
                        && !matches!(
                            svc.discovery_source.as_deref(),
                            Some("docker") | Some("podman")
                        );

                    if use_systemd {
                        probe::uptime::probe_systemd_unit(&probe_db, &svc, retention_days, &probe_tx)
                            .await;
                    } else if svc.url.is_some() {
                        let client = if svc.skip_tls_verify {
                            &probe_client_insecure
                        } else {
                            &probe_client
                        };
                        probe::uptime::probe_service(&probe_db, &svc, client, retention_days, &probe_tx)
                            .await;
                    }
                }
            });

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
                Some(new_id),
                Some(&detail),
                None,
            )
            .await;
            (StatusCode::CREATED, Json(json!({"id": new_id})))
        }
        Err(e) => {
            if e.as_database_error()
                .is_some_and(|de| de.is_unique_violation())
            {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({"error": "Unit already claimed"})),
                );
            }
            tracing::error!("Failed to create service: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create service"})),
            )
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/services/{id}",
    tag = "services",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Service ID"),
    ),
    request_body = UpdateService,
    responses(
        (status = 200, description = "Service updated"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Service not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn update_service(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateService>,
) -> impl IntoResponse {
    // Build dynamic update query
    // For simplicity, do a full update with fetched defaults
    let existing = sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible, skip_tls_verify, created_at, updated_at \
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
    let discovery_source = payload
        .discovery_source
        .unwrap_or(existing.discovery_source);
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
    let group_id = payload.group_id.unwrap_or(existing.group_id);
    let sort_order = payload.sort_order.unwrap_or(existing.sort_order);
    let probe_enabled = payload.probe_enabled.unwrap_or(existing.probe_enabled);
    let probe_interval = payload.probe_interval.unwrap_or(existing.probe_interval);
    let visible = payload.visible.unwrap_or(existing.visible);
    let skip_tls_verify = payload.skip_tls_verify.unwrap_or(existing.skip_tls_verify);
    let tags_json = match payload.tags {
        Some(t) => match serde_json::to_string(&t) {
            Ok(j) => Some(j),
            Err(e) => {
                tracing::error!("update_service: failed to serialize tags: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Internal error"})),
                );
            }
        },
        None => existing.tags,
    };

    let result = sqlx::query(
        "UPDATE services SET discovery_source = ?, display_name = ?, description = ?, url = ?, icon = ?, \
         group_id = ?, sort_order = ?, probe_enabled = ?, probe_interval = ?, \
         tags = ?, visible = ?, skip_tls_verify = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
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
    .bind(skip_tls_verify)
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

#[utoipa::path(
    delete,
    path = "/api/v1/services/{id}",
    tag = "services",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Service ID"),
    ),
    responses(
        (status = 200, description = "Service deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Service not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn delete_service(
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

#[utoipa::path(
    patch,
    path = "/api/v1/services/reorder",
    tag = "services",
    security(("cookieAuth" = [])),
    request_body = Vec<ReorderItem>,
    responses(
        (status = 200, description = "Sort orders updated"),
        (status = 400, description = "Empty reorder list"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn reorder_services(
    State(state): State<AppState>,
    session: Session,
    Json(items): Json<Vec<ReorderItem>>,
) -> impl IntoResponse {
    if items.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Reorder list is empty"})),
        );
    }

    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            );
        }
    };

    for item in &items {
        if let Err(e) = sqlx::query(
            "UPDATE services SET sort_order = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(item.sort_order)
        .bind(item.id)
        .execute(&mut *tx)
        .await
        {
            tracing::error!("Failed to update sort_order for service {}: {e}", item.id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            );
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit reorder transaction: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        );
    }

    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    let detail = serde_json::json!({"count": items.len()}).to_string();
    db::audit::insert(
        &state.db,
        &actor,
        "service.reorder",
        Some("service"),
        None,
        Some(&detail),
        None,
    )
    .await;

    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "reordered"})),
    )
}

/// Claim a discovered systemd unit — copies it into the services table with user-provided metadata.
#[utoipa::path(
    post,
    path = "/api/v1/services/{id}/claim",
    tag = "services",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Discovery unit ID (unused; payload drives insert)"),
    ),
    request_body = CreateService,
    responses(
        (status = 201, description = "Unit claimed and added to services"),
        (status = 401, description = "Not authenticated"),
        (status = 409, description = "Unit already claimed"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn claim_service(
    State(state): State<AppState>,
    session: Session,
    Path(_id): Path<i64>,
    Json(payload): Json<CreateService>,
) -> axum::response::Response {
    // Reuse create logic (dedup check + audit entry both handled there).
    create_service(State(state), session, Json(payload))
        .await
        .into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/services/{id}/start",
    tag = "services",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Start requested"),
        (status = 400, description = "Service has no systemd unit or container to control"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Service not found"),
        (status = 502, description = "The underlying systemd/Docker call failed"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn start_service(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> axum::response::Response {
    control_service(state, session, id, UnitAction::Start).await
}

#[utoipa::path(
    post,
    path = "/api/v1/services/{id}/stop",
    tag = "services",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Stop requested"),
        (status = 400, description = "Service has no systemd unit or container to control"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Service not found"),
        (status = 502, description = "The underlying systemd/Docker call failed"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn stop_service(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> axum::response::Response {
    control_service(state, session, id, UnitAction::Stop).await
}

#[utoipa::path(
    post,
    path = "/api/v1/services/{id}/restart",
    tag = "services",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Restart requested"),
        (status = 400, description = "Service has no systemd unit or container to control"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Service not found"),
        (status = 502, description = "The underlying systemd/Docker call failed"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn restart_service(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> axum::response::Response {
    control_service(state, session, id, UnitAction::Restart).await
}

/// Shared body for start/stop/restart: look up the service server-side (the
/// client only ever sends an id, never a unit/container name directly),
/// dispatch to the systemd or Docker backend, audit the attempt either way,
/// and fire an immediate re-probe on success so the dashboard reflects the
/// new state within one probe round-trip.
async fn control_service(
    state: AppState,
    session: Session,
    id: i64,
    action: UnitAction,
) -> axum::response::Response {
    let svc = match sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible, skip_tls_verify, created_at, updated_at \
         FROM services WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Service not found"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch service {id} for control action: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            )
                .into_response();
        }
    };

    let is_container = matches!(
        svc.discovery_source.as_deref(),
        Some("docker") | Some("podman")
    );

    let result: anyhow::Result<()> = if is_container {
        match &svc.systemd_unit {
            Some(name) => {
                let socket = state
                    .config
                    .docker
                    .sockets
                    .iter()
                    .map(|s| s.as_str())
                    .find(|s| {
                        let source = if s.contains("podman") {
                            "podman"
                        } else {
                            "docker"
                        };
                        Some(source) == svc.discovery_source.as_deref()
                    });
                match socket {
                    Some(socket) => control::docker::control_container(socket, name, action).await,
                    None => Err(anyhow::anyhow!(
                        "No configured Docker/Podman socket matches this service's discovery source"
                    )),
                }
            }
            None => Err(anyhow::anyhow!("Service has no container name recorded")),
        }
    } else if let Some(unit) = &svc.systemd_unit {
        control::systemd::control_unit(unit, action).await
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "This service isn't backed by a systemd unit or container and can't be controlled."
            })),
        )
            .into_response();
    };

    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());

    match result {
        Ok(()) => {
            db::audit::insert(
                &state.db,
                &actor,
                action.audit_action(),
                Some("service"),
                Some(id),
                Some(&json!({"display_name": svc.display_name}).to_string()),
                None,
            )
            .await;

            // Immediate re-probe so the dashboard reflects the new state soon,
            // mirroring the pattern already used after `create_service`.
            let probe_db = state.db.clone();
            let probe_tx = state.probe_tx.clone();
            let probe_client = state.probe_client.clone();
            let probe_client_insecure = state.probe_client_insecure.clone();
            let retention_days = state.config.probe.history_retention_days;
            let svc_for_probe = svc.clone();
            tokio::spawn(async move {
                let use_systemd = svc_for_probe.systemd_unit.is_some()
                    && !matches!(
                        svc_for_probe.discovery_source.as_deref(),
                        Some("docker") | Some("podman")
                    );
                if use_systemd {
                    probe::uptime::probe_systemd_unit(
                        &probe_db,
                        &svc_for_probe,
                        retention_days,
                        &probe_tx,
                    )
                    .await;
                } else if svc_for_probe.url.is_some() {
                    let client = if svc_for_probe.skip_tls_verify {
                        &probe_client_insecure
                    } else {
                        &probe_client
                    };
                    probe::uptime::probe_service(
                        &probe_db,
                        &svc_for_probe,
                        client,
                        retention_days,
                        &probe_tx,
                    )
                    .await;
                }
            });

            (StatusCode::OK, Json(json!({"status": "ok"}))).into_response()
        }
        Err(e) => {
            tracing::warn!("service control action {action:?} failed for service {id}: {e}");
            db::audit::insert(
                &state.db,
                &actor,
                action.audit_action(),
                Some("service"),
                Some(id),
                Some(
                    &json!({"display_name": svc.display_name, "error": e.to_string()}).to_string(),
                ),
                None,
            )
            .await;
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

/// Live-tails a tracked service's backing unit or container. Admin-only —
/// stricter than the read-only history/uptime routes, since log output is
/// arbitrary text a service prints and occasionally isn't safe for a
/// viewer role to see. The client only ever sends an id; the server
/// resolves the unit/container itself, same as the control routes above.
#[utoipa::path(
    get,
    path = "/api/v1/services/{id}/logs/stream",
    tag = "services",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Server-sent event stream of raw log lines (text/event-stream)",
         content_type = "text/event-stream"),
        (status = 400, description = "Service has no systemd unit or container to tail"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Service not found"),
        (status = 502, description = "Failed to start the log stream"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn service_logs_stream(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let svc = match sqlx::query_as::<_, Service>(
        "SELECT id, systemd_unit, discovery_source, display_name, description, url, icon, group_id, \
         sort_order, probe_enabled, probe_interval, tags, visible, skip_tls_verify, created_at, updated_at \
         FROM services WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Service not found"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch service {id} for log stream: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            )
                .into_response();
        }
    };

    let is_container = matches!(
        svc.discovery_source.as_deref(),
        Some("docker") | Some("podman")
    );

    let result: Result<BoxedLogStream, String> = if is_container {
        match &svc.systemd_unit {
            Some(name) => {
                let socket = state
                    .config
                    .docker
                    .sockets
                    .iter()
                    .map(|s| s.as_str())
                    .find(|s| {
                        let source = if s.contains("podman") {
                            "podman"
                        } else {
                            "docker"
                        };
                        Some(source) == svc.discovery_source.as_deref()
                    });
                match socket {
                    Some(socket) => control::docker::tail_container_logs(socket, name)
                        .await
                        .map(to_sse_log_stream)
                        .map_err(|e| e.to_string()),
                    None => Err(
                        "No configured Docker/Podman socket matches this service's discovery source"
                            .to_string(),
                    ),
                }
            }
            None => Err("Service has no container name recorded".to_string()),
        }
    } else if let Some(unit) = &svc.systemd_unit {
        control::systemd::tail_unit_logs(unit)
            .await
            .map(to_sse_log_stream)
            .map_err(|e| e.to_string())
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "This service isn't backed by a systemd unit or container and can't be controlled."
            })),
        )
            .into_response();
    };

    match result {
        Ok(stream) => Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
            .into_response(),
        Err(msg) => (StatusCode::BAD_GATEWAY, Json(json!({"error": msg}))).into_response(),
    }
}
