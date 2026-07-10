use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;

use crate::db;
use crate::db::models::{CreateQuickLinkGroup, QuickLinkGroup, UpdateQuickLinkGroup};
use crate::AppState;
use tower_sessions::Session;

pub fn read_router() -> Router<AppState> {
    Router::new().route("/", get(list_quick_link_groups))
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_quick_link_group))
        .route(
            "/{id}",
            put(update_quick_link_group).delete(delete_quick_link_group),
        )
}

#[utoipa::path(
    get,
    path = "/api/v1/quick-link-groups",
    tag = "quick-link-groups",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of all quick link groups", body = Vec<QuickLinkGroup>),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_quick_link_groups(State(state): State<AppState>) -> impl IntoResponse {
    let groups = sqlx::query_as::<_, QuickLinkGroup>(
        "SELECT id, name, icon, color, sort_order, created_at FROM quick_link_groups ORDER BY sort_order ASC",
    )
    .fetch_all(&state.db)
    .await;

    match groups {
        Ok(g) => (StatusCode::OK, Json(json!(g))),
        Err(e) => {
            tracing::error!("Failed to list quick link groups: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch quick link groups"})),
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/quick-link-groups",
    tag = "quick-link-groups",
    security(("cookieAuth" = [])),
    request_body = CreateQuickLinkGroup,
    responses(
        (status = 201, description = "Quick link group created; returns new ID"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn create_quick_link_group(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateQuickLinkGroup>,
) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO quick_link_groups (name, icon, color, sort_order) VALUES (?, ?, ?, ?)",
    )
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
                "quick_link_group.create",
                Some("quick_link_group"),
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
            tracing::error!("Failed to create quick link group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create quick link group"})),
            )
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/quick-link-groups/{id}",
    tag = "quick-link-groups",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Quick link group ID"),
    ),
    request_body = UpdateQuickLinkGroup,
    responses(
        (status = 200, description = "Quick link group updated"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Quick link group not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn update_quick_link_group(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateQuickLinkGroup>,
) -> impl IntoResponse {
    let existing = sqlx::query_as::<_, QuickLinkGroup>(
        "SELECT id, name, icon, color, sort_order, created_at FROM quick_link_groups WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let existing = match existing {
        Ok(Some(g)) => g,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Quick link group not found"})),
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
    let color = payload.color.or(existing.color);
    let sort_order = payload.sort_order.unwrap_or(existing.sort_order);

    let result = sqlx::query(
        "UPDATE quick_link_groups SET name = ?, icon = ?, color = ?, sort_order = ? WHERE id = ?",
    )
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
                "quick_link_group.update",
                Some("quick_link_group"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "updated"})))
        }
        Err(e) => {
            tracing::error!("Failed to update quick link group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update quick link group"})),
            )
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/quick-link-groups/{id}",
    tag = "quick-link-groups",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "Quick link group ID"),
    ),
    responses(
        (status = 200, description = "Quick link group deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Quick link group not found"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn delete_quick_link_group(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM quick_link_groups WHERE id = ?")
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
                "quick_link_group.delete",
                Some("quick_link_group"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "deleted"})))
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Quick link group not found"})),
        ),
        Err(e) => {
            tracing::error!("Failed to delete quick link group: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to delete quick link group"})),
            )
        }
    }
}
