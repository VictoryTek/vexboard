use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;

use crate::db;
use crate::db::models::{CreateGroup, Group, UpdateGroup};
use crate::AppState;
use tower_sessions::Session;

pub fn read_router() -> Router<AppState> {
    Router::new().route("/", get(list_groups))
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_group))
        .route("/{id}", put(update_group).delete(delete_group))
}

#[utoipa::path(
    get,
    path = "/api/v1/groups",
    tag = "groups",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of all groups", body = Vec<Group>),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_groups(State(state): State<AppState>) -> impl IntoResponse {
    let groups = sqlx::query_as::<_, Group>(
        "SELECT id, name, icon, color, sort_order, created_at FROM groups ORDER BY sort_order ASC",
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

#[utoipa::path(
    post,
    path = "/api/v1/groups",
    tag = "groups",
    security(("cookieAuth" = [])),
    request_body = CreateGroup,
    responses(
        (status = 201, description = "Group created; returns new ID"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn create_group(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateGroup>,
) -> impl IntoResponse {
    let result =
        sqlx::query("INSERT INTO groups (name, icon, color, sort_order) VALUES (?, ?, ?, ?)")
            .bind(&payload.name)
            .bind(&payload.icon)
            .bind(&payload.color)
            .bind(payload.sort_order.unwrap_or(0))
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
            let detail = serde_json::json!({"name": payload.name}).to_string();
            db::audit::insert(
                &state.db,
                &actor,
                "group.create",
                Some("group"),
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
            tracing::error!("Failed to create group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create group"})),
            )
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/groups/{id}",
    tag = "groups",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Group ID"),
    ),
    request_body = UpdateGroup,
    responses(
        (status = 200, description = "Group updated"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Group not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn update_group(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateGroup>,
) -> impl IntoResponse {
    let existing = sqlx::query_as::<_, Group>(
        "SELECT id, name, icon, color, sort_order, created_at FROM groups WHERE id = ?",
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
    let icon = payload.icon.unwrap_or(existing.icon);
    let color = payload.color.unwrap_or(existing.color);
    let sort_order = payload.sort_order.unwrap_or(existing.sort_order);

    let result =
        sqlx::query("UPDATE groups SET name = ?, icon = ?, color = ?, sort_order = ? WHERE id = ?")
            .bind(&name)
            .bind(&icon)
            .bind(&color)
            .bind(sort_order)
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
                "group.update",
                Some("group"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "updated"})))
        }
        Err(e) => {
            tracing::error!("Failed to update group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update group"})),
            )
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/groups/{id}",
    tag = "groups",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Group ID"),
    ),
    responses(
        (status = 200, description = "Group deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Group not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn delete_group(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM groups WHERE id = ?")
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
                "group.delete",
                Some("group"),
                Some(id),
                None,
                None,
            )
            .await;
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
