use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::json;
use tower_sessions::Session;

use crate::db;
use crate::db::models::{CreateUserRequest, UpdateUserRequest, UserPublic};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route(
            "/{id}",
            axum::routing::patch(update_user).delete(delete_user),
        )
}

#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of all users", body = Vec<UserPublic>),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_users(State(state): State<AppState>) -> impl IntoResponse {
    let users = sqlx::query_as::<_, UserPublic>(
        "SELECT id, username, role, created_at FROM users ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await;

    match users {
        Ok(u) => (StatusCode::OK, Json(json!(u))),
        Err(e) => {
            tracing::error!("Failed to list users: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch users"})),
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/users",
    tag = "users",
    security(("cookieAuth" = [])),
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created; returns new ID"),
        (status = 400, description = "Validation error or invalid role"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 409, description = "Username already taken"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn create_user(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateUserRequest>,
) -> impl IntoResponse {
    if payload.username.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Username cannot be empty"})),
        );
    }
    if payload.password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Password must be at least 8 characters"})),
        );
    }
    if payload.role != "admin" && payload.role != "viewer" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Role must be 'admin' or 'viewer'"})),
        );
    }

    let taken: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(&payload.username)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    if taken.is_some() {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "Username already taken"})),
        );
    }

    let hash = match bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("bcrypt error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Internal error"})),
            );
        }
    };

    let result = sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?)")
        .bind(&payload.username)
        .bind(&hash)
        .bind(&payload.role)
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
            let detail =
                serde_json::json!({"username": payload.username, "role": payload.role}).to_string();
            db::audit::insert(
                &state.db,
                &actor,
                "user.create",
                Some("user"),
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
            tracing::error!("Failed to create user: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create user"})),
            )
        }
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/users/{id}",
    tag = "users",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "User ID"),
    ),
    request_body = UpdateUserRequest,
    responses(
        (status = 200, description = "User updated"),
        (status = 400, description = "Validation error or invalid role"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required or self-demotion blocked"),
        (status = 404, description = "User not found"),
        (status = 409, description = "Cannot demote the last admin"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn update_user(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateUserRequest>,
) -> impl IntoResponse {
    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());

    let target = sqlx::query_as::<_, UserPublic>(
        "SELECT id, username, role, created_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let target = match target {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "User not found"})),
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

    // Block self-demotion
    if target.username == actor {
        if let Some(ref new_role) = payload.role {
            if new_role != "admin" {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "Cannot demote your own account"})),
                );
            }
        }
    }

    // Guard against removing the last admin
    if let Some(ref new_role) = payload.role {
        if new_role != "admin" && target.role == "admin" {
            let admin_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap_or(2);
            if admin_count <= 1 {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({"error": "Cannot demote the last admin"})),
                );
            }
        }
        if new_role != "admin" && new_role != "viewer" {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Role must be 'admin' or 'viewer'"})),
            );
        }
    }

    let new_role = payload.role.as_deref().unwrap_or(&target.role);
    let new_username = payload.username.as_deref().unwrap_or(&target.username);

    // Check username uniqueness if changing
    if new_username != target.username {
        let taken: Option<i64> =
            sqlx::query_scalar("SELECT id FROM users WHERE username = ? AND id != ?")
                .bind(new_username)
                .bind(id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);
        if taken.is_some() {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "Username already taken"})),
            );
        }
    }

    let result = sqlx::query("UPDATE users SET username = ?, role = ? WHERE id = ?")
        .bind(new_username)
        .bind(new_role)
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
            let detail =
                serde_json::json!({"role": new_role, "username": new_username}).to_string();
            db::audit::insert(
                &state.db,
                &actor,
                "user.update",
                Some("user"),
                Some(id),
                Some(&detail),
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "updated"})))
        }
        Err(e) => {
            tracing::error!("Failed to update user: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to update user"})),
            )
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/users/{id}",
    tag = "users",
    security(("cookieAuth" = [])),
    params(
        ("id" = i64, Path, description = "User ID"),
    ),
    responses(
        (status = 200, description = "User deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "User not found"),
        (status = 409, description = "Cannot delete own account or last admin"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session))]
pub(crate) async fn delete_user(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());

    let target = sqlx::query_as::<_, UserPublic>(
        "SELECT id, username, role, created_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    let target = match target {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "User not found"})),
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

    if target.username == actor {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "Cannot delete your own account"})),
        );
    }

    if target.role == "admin" {
        let admin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
                .fetch_one(&state.db)
                .await
                .unwrap_or(2);
        if admin_count <= 1 {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "Cannot delete the last admin"})),
            );
        }
    }

    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            db::audit::insert(
                &state.db,
                &actor,
                "user.delete",
                Some("user"),
                Some(id),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "deleted"})))
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        ),
        Err(e) => {
            tracing::error!("Failed to delete user: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to delete user"})),
            )
        }
    }
}
