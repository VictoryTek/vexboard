use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::json;

use crate::db::models::{CreateGroup, Group, UpdateGroup};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_groups).post(create_group))
        .route("/:id", put(update_group).delete(delete_group))
}

#[tracing::instrument(skip(state))]
async fn list_groups(State(state): State<AppState>) -> impl IntoResponse {
    let groups = sqlx::query_as::<_, Group>(
        "SELECT id, name, icon, sort_order, created_at FROM groups ORDER BY sort_order ASC",
    )
    .fetch_all(&state.db)
    .await;

    match groups {
        Ok(g) => (StatusCode::OK, Json(json!(g))),
        Err(e) => {
            tracing::error!("Failed to list groups: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch groups"})),
            )
        }
    }
}

#[tracing::instrument(skip(state))]
async fn create_group(
    State(state): State<AppState>,
    Json(payload): Json<CreateGroup>,
) -> impl IntoResponse {
    let result = sqlx::query("INSERT INTO groups (name, icon, sort_order) VALUES (?, ?, ?)")
        .bind(&payload.name)
        .bind(&payload.icon)
        .bind(payload.sort_order.unwrap_or(0))
        .execute(&state.db)
        .await;

    match result {
        Ok(r) => (
            StatusCode::CREATED,
            Json(json!({"id": r.last_insert_rowid()})),
        ),
        Err(e) => {
            tracing::error!("Failed to create group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create group"})),
            )
        }
    }
}

#[tracing::instrument(skip(state))]
async fn update_group(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateGroup>,
) -> impl IntoResponse {
    let existing = sqlx::query_as::<_, Group>(
        "SELECT id, name, icon, sort_order, created_at FROM groups WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let existing = match existing {
        Ok(Some(g)) => g,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Group not found"})),
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

    let name = payload.name.unwrap_or(existing.name);
    let icon = payload.icon.or(existing.icon);
    let sort_order = payload.sort_order.unwrap_or(existing.sort_order);

    let result = sqlx::query("UPDATE groups SET name = ?, icon = ?, sort_order = ? WHERE id = ?")
        .bind(&name)
        .bind(&icon)
        .bind(sort_order)
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "updated"}))),
        Err(e) => {
            tracing::error!("Failed to update group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update group"})),
            )
        }
    }
}

#[tracing::instrument(skip(state))]
async fn delete_group(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM groups WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            (StatusCode::OK, Json(json!({"status": "deleted"})))
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Group not found"})),
        ),
        Err(e) => {
            tracing::error!("Failed to delete group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to delete group"})),
            )
        }
    }
}
