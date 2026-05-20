use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::db::models::{LoginRequest, UserInfo};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[tracing::instrument(skip_all)]
async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    let user = sqlx::query_as::<_, crate::db::models::User>(
        "SELECT id, username, password_hash, created_at FROM users WHERE username = ?",
    )
    .bind(&payload.username)
    .fetch_optional(&state.db)
    .await;

    let user = match user {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Invalid credentials"})),
            )
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            )
        }
    };

    let valid = bcrypt::verify(&payload.password, &user.password_hash).unwrap_or(false);
    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        );
    }

    // In a full implementation, create a session via tower-sessions here.
    // For now, return success with user info.
    (
        StatusCode::OK,
        Json(json!({
            "user": UserInfo { id: user.id, username: user.username }
        })),
    )
}

#[tracing::instrument(skip_all)]
async fn logout() -> impl IntoResponse {
    // Invalidate session (tower-sessions integration)
    (StatusCode::OK, Json(json!({"status": "logged out"})))
}

#[tracing::instrument(skip_all)]
async fn me() -> impl IntoResponse {
    // Return current session user (placeholder until session middleware is wired)
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "Not authenticated"})),
    )
}
