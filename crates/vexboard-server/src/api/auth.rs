use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use tower_sessions::Session;

use crate::db;
use crate::db::models::LoginRequest;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me).patch(update_me))
}

/// Extract the client IP from ConnectInfo, falling back to X-Forwarded-For.
fn client_ip(connect_info: &ConnectInfo<SocketAddr>, headers: &HeaderMap) -> IpAddr {
    if let Some(forwarded) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
    {
        return forwarded;
    }
    connect_info.0.ip()
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
    State(state): State<AppState>,
    connect_info: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    session: Session,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    if state.config.auth.login_rate_limit_attempts > 0 {
        let ip = client_ip(&connect_info, &headers);
        if !state.login_limiter.check(ip) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({"error": "Too many login attempts — try again later"})),
            );
        }
    }
    let ip = client_ip(&connect_info, &headers);
    let ip_str = ip.to_string();
    use crate::pam_auth::authenticate_pam;
    if authenticate_pam(&payload.username, &payload.password) {
        if let Err(e) = session.insert("username", payload.username.clone()).await {
            tracing::error!("failed to persist session after login: {e}");
        }
        db::audit::insert(
            &state.db,
            &payload.username,
            "auth.login_success",
            None,
            None,
            None,
            Some(&ip_str),
        )
        .await;
        (
            StatusCode::OK,
            Json(json!({ "user": { "username": payload.username } })),
        )
    } else {
        let detail = serde_json::json!({"username": payload.username}).to_string();
        db::audit::insert(
            &state.db,
            &payload.username,
            "auth.login_failure",
            None,
            None,
            Some(&detail),
            Some(&ip_str),
        )
        .await;
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
    connect_info: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    session: Session,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    let ip = client_ip(&connect_info, &headers);
    let ip_str = ip.to_string();
    if state.config.auth.login_rate_limit_attempts > 0 && !state.login_limiter.check(ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "Too many login attempts — try again later"})),
        );
    }
    let user = sqlx::query_as::<_, crate::db::models::User>(
        "SELECT id, username, password_hash, created_at FROM users WHERE username = ?",
    )
    .bind(&payload.username)
    .fetch_optional(&state.db)
    .await;

    let user = match user {
        Ok(Some(u)) => u,
        Ok(None) => {
            let detail = serde_json::json!({"username": payload.username}).to_string();
            db::audit::insert(
                &state.db,
                &payload.username,
                "auth.login_failure",
                None,
                None,
                Some(&detail),
                Some(&ip_str),
            )
            .await;
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Invalid credentials"})),
            );
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
        let detail = serde_json::json!({"username": payload.username}).to_string();
        db::audit::insert(
            &state.db,
            &payload.username,
            "auth.login_failure",
            None,
            None,
            Some(&detail),
            Some(&ip_str),
        )
        .await;
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        );
    }

    if let Err(e) = session.insert("username", user.username.clone()).await {
        tracing::error!("failed to persist session after login: {e}");
    }
    db::audit::insert(
        &state.db,
        &user.username,
        "auth.login_success",
        None,
        None,
        None,
        Some(&ip_str),
    )
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "user": crate::db::models::UserInfo { id: user.id, username: user.username }
        })),
    )
}

#[tracing::instrument(skip_all)]
async fn logout(State(state): State<AppState>, session: Session) -> impl IntoResponse {
    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    session.flush().await.ok();
    db::audit::insert(&state.db, &actor, "auth.logout", None, None, None, None).await;
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

    db::audit::insert(
        &state.db,
        &current_username,
        "auth.credential_change",
        Some("user"),
        None,
        None,
        None,
    )
    .await;
    session.flush().await.ok();

    (StatusCode::OK, Json(json!({"ok": true})))
}
