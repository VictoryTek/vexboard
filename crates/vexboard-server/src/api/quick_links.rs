use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::db::models::{CreateQuickLink, QuickLink, UpdateQuickLink};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_quick_links).post(create_quick_link))
        .route("/{id}", axum::routing::put(update_quick_link).delete(delete_quick_link))
}

async fn list_quick_links(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_as::<_, QuickLink>(
        "SELECT id, title, url, icon, description, sort_order FROM quick_links ORDER BY sort_order ASC, id ASC",
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

async fn create_quick_link(
    State(state): State<AppState>,
    Json(payload): Json<CreateQuickLink>,
) -> impl IntoResponse {
    match sqlx::query(
        "INSERT INTO quick_links (title, url, icon, description, sort_order) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&payload.title)
    .bind(&payload.url)
    .bind(&payload.icon)
    .bind(&payload.description)
    .bind(payload.sort_order.unwrap_or(0))
    .execute(&state.db)
    .await
    {
        Ok(r) => (StatusCode::CREATED, Json(json!({"id": r.last_insert_rowid()}))),
        Err(e) => {
            tracing::error!("Failed to create quick link: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to create quick link"})))
        }
    }
}

async fn update_quick_link(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateQuickLink>,
) -> impl IntoResponse {
    let existing = sqlx::query_as::<_, QuickLink>(
        "SELECT id, title, url, icon, description, sort_order FROM quick_links WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let existing = match existing {
        Ok(Some(l)) => l,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
        Err(e) => {
            tracing::error!("DB error: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"})));
        }
    };

    let title = payload.title.unwrap_or(existing.title);
    let url = payload.url.unwrap_or(existing.url);
    let icon = payload.icon.map(|v| if v.is_empty() { None } else { Some(v) }).unwrap_or(existing.icon);
    let description = payload.description.map(|v| if v.is_empty() { None } else { Some(v) }).unwrap_or(existing.description);
    let sort_order = payload.sort_order.unwrap_or(existing.sort_order);

    match sqlx::query(
        "UPDATE quick_links SET title = ?, url = ?, icon = ?, description = ?, sort_order = ?, \
         updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&title)
    .bind(&url)
    .bind(&icon)
    .bind(&description)
    .bind(sort_order)
    .bind(id)
    .execute(&state.db)
    .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "updated"}))),
        Err(e) => {
            tracing::error!("Failed to update quick link: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to update quick link"})))
        }
    }
}

async fn delete_quick_link(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match sqlx::query("DELETE FROM quick_links WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"status": "deleted"}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
        Err(e) => {
            tracing::error!("Failed to delete quick link: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to delete quick link"})))
        }
    }
}
