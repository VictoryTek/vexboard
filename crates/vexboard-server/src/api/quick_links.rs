use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use serde_json::json;

use crate::db;
use crate::db::models::{CreateQuickLink, QuickLink, ReorderItem, UpdateQuickLink};
use crate::AppState;
use tower_sessions::Session;

pub fn read_router() -> Router<AppState> {
    Router::new().route("/", get(list_quick_links))
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_quick_link))
        .route(
            "/{id}",
            axum::routing::put(update_quick_link).delete(delete_quick_link),
        )
        .route("/reorder", patch(reorder_quick_links))
}

#[utoipa::path(
    get,
    path = "/api/v1/quick-links",
    tag = "quick-links",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of all quick links", body = Vec<QuickLink>),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
pub(crate) async fn list_quick_links(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_as::<_, QuickLink>(
        "SELECT id, title, url, icon, description, group_id, sort_order FROM quick_links ORDER BY sort_order ASC, id ASC",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(links) => (StatusCode::OK, Json(json!(links))),
        Err(e) => {
            tracing::error!("Failed to list quick links: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to fetch quick links"})))
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/quick-links",
    tag = "quick-links",
    security(("cookieAuth" = [])),
    request_body = CreateQuickLink,
    responses(
        (status = 201, description = "Quick link created; returns new ID"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
pub(crate) async fn create_quick_link(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateQuickLink>,
) -> impl IntoResponse {
    match sqlx::query(
        "INSERT INTO quick_links (title, url, icon, description, group_id, sort_order) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&payload.title)
    .bind(&payload.url)
    .bind(&payload.icon)
    .bind(&payload.description)
    .bind(payload.group_id)
    .bind(payload.sort_order.unwrap_or(0))
    .execute(&state.db)
    .await
    {
        Ok(r) => {
            let actor = session.get::<String>("username").await.ok().flatten().unwrap_or_else(|| "unknown".to_string());
            let detail = serde_json::json!({"title": payload.title}).to_string();
            db::audit::insert(&state.db, &actor, "quick_link.create", Some("quick_link"), Some(r.last_insert_rowid()), Some(&detail), None).await;
            (StatusCode::CREATED, Json(json!({"id": r.last_insert_rowid()})))
        }
        Err(e) => {
            tracing::error!("Failed to create quick link: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to create quick link"})))
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/quick-links/{id}",
    tag = "quick-links",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Quick link ID"),
    ),
    request_body = UpdateQuickLink,
    responses(
        (status = 200, description = "Quick link updated"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Quick link not found"),
        (status = 500, description = "Database error"),
    )
)]
pub(crate) async fn update_quick_link(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateQuickLink>,
) -> impl IntoResponse {
    let existing = sqlx::query_as::<_, QuickLink>(
        "SELECT id, title, url, icon, description, group_id, sort_order FROM quick_links WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let existing = match existing {
        Ok(Some(l)) => l,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
        Err(e) => {
            tracing::error!("DB error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            );
        }
    };

    let title = payload.title.unwrap_or(existing.title);
    let url = payload.url.unwrap_or(existing.url);
    let icon = payload
        .icon
        .map(|v| if v.is_empty() { None } else { Some(v) })
        .unwrap_or(existing.icon);
    let description = payload
        .description
        .map(|v| if v.is_empty() { None } else { Some(v) })
        .unwrap_or(existing.description);
    let group_id = payload.group_id.unwrap_or(existing.group_id);
    let sort_order = payload.sort_order.unwrap_or(existing.sort_order);

    match sqlx::query(
        "UPDATE quick_links SET title = ?, url = ?, icon = ?, description = ?, group_id = ?, sort_order = ?, \
         updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&title)
    .bind(&url)
    .bind(&icon)
    .bind(&description)
    .bind(group_id)
    .bind(sort_order)
    .bind(id)
    .execute(&state.db)
    .await
    {
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
                "quick_link.update",
                Some("quick_link"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "updated"})))
        }
        Err(e) => {
            tracing::error!("Failed to update quick link: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update quick link"})),
            )
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/quick-links/{id}",
    tag = "quick-links",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Quick link ID"),
    ),
    responses(
        (status = 200, description = "Quick link deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Quick link not found"),
        (status = 500, description = "Database error"),
    )
)]
pub(crate) async fn delete_quick_link(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match sqlx::query("DELETE FROM quick_links WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
    {
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
                "quick_link.delete",
                Some("quick_link"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "deleted"})))
        }
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
        Err(e) => {
            tracing::error!("Failed to delete quick link: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to delete quick link"})),
            )
        }
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/quick-links/reorder",
    tag = "quick-links",
    security(("cookieAuth" = [])),
    request_body = Vec<ReorderItem>,
    responses(
        (status = 200, description = "Sort orders updated"),
        (status = 400, description = "Empty reorder list"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
pub(crate) async fn reorder_quick_links(
    State(state): State<AppState>,
    session: Session,
    Json(items): Json<Vec<ReorderItem>>,
) -> impl IntoResponse {
    if items.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Reorder list is empty"})),
        );
    }

    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            );
        }
    };

    for item in &items {
        if let Err(e) = sqlx::query(
            "UPDATE quick_links SET sort_order = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(item.sort_order)
        .bind(item.id)
        .execute(&mut *tx)
        .await
        {
            tracing::error!(
                "Failed to update sort_order for quick link {}: {e}",
                item.id
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            );
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit reorder transaction: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Database error"})),
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
        "quick_link.reorder",
        Some("quick_link"),
        None,
        Some(&detail),
        None,
    )
    .await;

    (StatusCode::OK, Json(json!({"status": "reordered"})))
}
