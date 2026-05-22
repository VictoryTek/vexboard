use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

#[cfg(feature = "pam-auth")]
pub async fn status() -> impl axum::response::IntoResponse {
    (
        StatusCode::OK,
        Json(json!({ "needs_setup": false, "auth_mode": "pam" })),
    )
}

#[cfg(not(feature = "pam-auth"))]
pub async fn status(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .unwrap_or(1);
    (
        StatusCode::OK,
        Json(json!({ "needs_setup": count == 0, "auth_mode": "local" })),
    )
}

#[cfg(not(feature = "pam-auth"))]
pub async fn create_admin(
    State(state): State<AppState>,
    Json(payload): Json<SetupRequest>,
) -> impl axum::response::IntoResponse {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .unwrap_or(1);
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
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ok"}))),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to create user"})),
        ),
    }
}

#[cfg(feature = "pam-auth")]
pub async fn create_admin() -> impl axum::response::IntoResponse {
    (
        StatusCode::GONE,
        Json(json!({"error": "Not applicable in PAM mode"})),
    )
}
