use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tower_sessions::Session;

use crate::db::models::LoginRequest;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[cfg(all(unix, feature = "pam-auth"))]
#[tracing::instrument(skip_all)]
async fn login(
    State(_state): State<AppState>,
    session: Session,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    use crate::pam_auth::authenticate_pam;
    if authenticate_pam(&payload.username, &payload.password) {
        session
            .insert("username", payload.username.clone())
            .await
            .ok();
        (
            StatusCode::OK,
            Json(json!({ "user": { "username": payload.username } })),
        )
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        )
    }
}

#[cfg(not(all(unix, feature = "pam-auth")))]
#[tracing::instrument(skip_all)]
async fn login(
    State(state): State<AppState>,
    session: Session,
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

    session.insert("username", user.username.clone()).await.ok();

    (
        StatusCode::OK,
        Json(json!({
            "user": crate::db::models::UserInfo { id: user.id, username: user.username }
        })),
    )
}

#[tracing::instrument(skip_all)]
async fn logout(session: Session) -> impl IntoResponse {
    session.flush().await.ok();
    (StatusCode::OK, Json(json!({"status": "logged out"})))
}

#[tracing::instrument(skip_all)]
async fn me(session: Session) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(username)) => (
            StatusCode::OK,
            Json(json!({ "user": { "username": username } })),
        ),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Not authenticated"})),
        ),
    }
}
