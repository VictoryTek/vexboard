use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::json;

use crate::db;
use crate::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

#[cfg(feature = "pam-auth")]
#[utoipa::path(
    get,
    path = "/api/v1/setup/status",
    tag = "setup",
    responses(
        (status = 200, description = "Setup status"),
    )
)]
pub async fn status() -> impl axum::response::IntoResponse {
    (
        StatusCode::OK,
        Json(json!({ "needs_setup": false, "auth_mode": "pam" })),
    )
}

#[cfg(not(feature = "pam-auth"))]
#[utoipa::path(
    get,
    path = "/api/v1/setup/status",
    tag = "setup",
    responses(
        (status = 200, description = "Returns whether the initial admin setup is required"),
    )
)]
pub async fn status(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    let count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("setup/status: failed to query user count: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Internal error"})),
            );
        }
    };
    (
        StatusCode::OK,
        Json(json!({ "needs_setup": count == 0, "auth_mode": "local" })),
    )
}

#[cfg(not(feature = "pam-auth"))]
#[utoipa::path(
    post,
    path = "/api/v1/setup",
    tag = "setup",
    request_body = SetupRequest,
    responses(
        (status = 200, description = "Admin account created successfully"),
        (status = 400, description = "Invalid username or password"),
        (status = 409, description = "Setup already completed"),
        (status = 500, description = "Internal error"),
    )
)]
pub async fn create_admin(
    State(state): State<AppState>,
    Json(payload): Json<SetupRequest>,
) -> impl axum::response::IntoResponse {
    let count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("setup/create_admin: failed to query user count: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Internal error"})),
            );
        }
    };
    if count != 0 {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "Setup already completed"})),
        );
    }
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
    let hash = match bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Internal error"})),
            )
        }
    };
    match sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
        .bind(&payload.username)
        .bind(&hash)
        .execute(&state.db)
        .await
    {
        Ok(_) => {
            db::audit::insert(
                &state.db,
                &payload.username,
                "setup.admin_created",
                Some("user"),
                None,
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({"status": "ok"})))
        }
        Err(e) => {
            // A concurrent setup request won the race — the UNIQUE constraint
            // on username fired after we observed count == 0.
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.message().contains("UNIQUE constraint failed") {
                    return (
                        StatusCode::CONFLICT,
                        Json(json!({"error": "Setup already completed"})),
                    );
                }
            }
            tracing::error!("setup/create_admin: failed to insert admin user: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create user"})),
            )
        }
    }
}

#[cfg(feature = "pam-auth")]
#[utoipa::path(
    post,
    path = "/api/v1/setup",
    tag = "setup",
    responses(
        (status = 410, description = "Not applicable in PAM mode"),
    )
)]
pub async fn create_admin() -> impl axum::response::IntoResponse {
    (
        StatusCode::GONE,
        Json(json!({"error": "Not applicable in PAM mode"})),
    )
}
