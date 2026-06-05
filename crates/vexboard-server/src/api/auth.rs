use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tower_sessions::Session;

use crate::db::models::LoginRequest;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me).patch(update_me))
}

#[derive(Debug, Deserialize)]
struct UpdateMeRequest {
    current_password: String,
    new_username: Option<String>,
    new_password: Option<String>,
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
        if let Err(e) = session.insert("username", payload.username.clone()).await {
            tracing::error!("failed to persist session after login: {e}");
        }
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

    if let Err(e) = session.insert("username", user.username.clone()).await {
        tracing::error!("failed to persist session after login: {e}");
    }

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

#[cfg(all(unix, feature = "pam-auth"))]
#[tracing::instrument(skip_all)]
async fn me(session: Session) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(username)) => (
            StatusCode::OK,
            Json(json!({ "user": { "username": username, "auth_mode": "pam" } })),
        ),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Not authenticated"})),
        ),
    }
}

#[cfg(not(all(unix, feature = "pam-auth")))]
#[tracing::instrument(skip_all)]
async fn me(session: Session) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(username)) => (
            StatusCode::OK,
            Json(json!({ "user": { "username": username, "auth_mode": "local" } })),
        ),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Not authenticated"})),
        ),
    }
}

#[cfg(all(unix, feature = "pam-auth"))]
#[tracing::instrument(skip_all)]
async fn update_me() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"error": "credential changes not supported in PAM auth mode"})),
    )
}

#[cfg(not(all(unix, feature = "pam-auth")))]
#[tracing::instrument(skip_all)]
async fn update_me(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<UpdateMeRequest>,
) -> impl IntoResponse {
    let current_username = match session.get::<String>("username").await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
        }
    };

    let user = match sqlx::query_as::<_, crate::db::models::User>(
        "SELECT id, username, password_hash, created_at FROM users WHERE username = ?",
    )
    .bind(&current_username)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            )
        }
    };

    let valid = bcrypt::verify(&payload.current_password, &user.password_hash).unwrap_or(false);
    if !valid {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Invalid current password"})),
        );
    }

    if let Some(ref s) = payload.new_username {
        if s.trim().is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "username cannot be empty"})),
            );
        }
    }
    if let Some(ref s) = payload.new_password {
        if s.len() < 8 {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "password must be at least 8 characters"})),
            );
        }
    }

    if let Some(ref new_username) = payload.new_username {
        let taken: Option<i64> =
            sqlx::query_scalar("SELECT id FROM users WHERE username = ? AND id != ?")
                .bind(new_username)
                .bind(user.id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

        if taken.is_some() {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "Username already taken"})),
            );
        }

        if sqlx::query("UPDATE users SET username = ? WHERE id = ?")
            .bind(new_username)
            .bind(user.id)
            .execute(&state.db)
            .await
            .is_err()
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            );
        }
    }

    if let Some(ref new_password) = payload.new_password {
        let hashed = match bcrypt::hash(new_password, bcrypt::DEFAULT_COST) {
            Ok(h) => h,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Failed to hash password"})),
                )
            }
        };

        if sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(&hashed)
            .bind(user.id)
            .execute(&state.db)
            .await
            .is_err()
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Database error"})),
            );
        }
    }

    session.flush().await.ok();

    (StatusCode::OK, Json(json!({"ok": true})))
}
