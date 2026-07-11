use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
#[cfg(not(all(unix, feature = "pam-auth")))]
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

/// Extract the client IP from ConnectInfo, honoring X-Forwarded-For's last hop
/// only when `behind_proxy` is set — the header is fully client-controlled
/// otherwise and must not be trusted.
fn client_ip(
    connect_info: &ConnectInfo<SocketAddr>,
    headers: &HeaderMap,
    behind_proxy: bool,
) -> IpAddr {
    if behind_proxy {
        if let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit(',').next())
            .and_then(|s| s.trim().parse::<IpAddr>().ok())
        {
            return forwarded;
        }
    }
    connect_info.0.ip()
}

#[cfg(not(all(unix, feature = "pam-auth")))]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct UpdateMeRequest {
    current_password: String,
    new_username: Option<String>,
    new_password: Option<String>,
}

// ---------------------------------------------------------------------------
// login
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful"),
        (status = 401, description = "Invalid credentials"),
        (status = 429, description = "Too many login attempts"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn login(
    State(state): State<AppState>,
    connect_info: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    session: Session,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    let ip = client_ip(&connect_info, &headers, state.config.auth.behind_proxy);
    let ip_str = ip.to_string();
    if state.config.auth.login_rate_limit_attempts > 0 && !state.login_limiter.check(ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "Too many login attempts — try again later"})),
        );
    }

    #[cfg(all(unix, feature = "pam-auth"))]
    return login_pam(&state, &session, &payload, &ip_str).await;

    #[cfg(not(all(unix, feature = "pam-auth")))]
    login_local(&state, &session, &payload, &ip_str).await
}

#[cfg(all(unix, feature = "pam-auth"))]
async fn login_pam(
    state: &AppState,
    session: &Session,
    payload: &LoginRequest,
    ip: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    use crate::pam_auth::authenticate_pam;
    if authenticate_pam(&payload.username, &payload.password) {
        if let Err(e) = session.cycle_id().await {
            tracing::error!("failed to cycle session id after login: {e}");
        }
        if let Err(e) = session.insert("username", payload.username.clone()).await {
            tracing::error!("failed to persist session after login: {e}");
        }
        if let Err(e) = session.insert("role", "admin".to_string()).await {
            tracing::error!("failed to persist role in session after login: {e}");
        }
        db::audit::insert(
            &state.db,
            &payload.username,
            "auth.login_success",
            None,
            None,
            None,
            Some(ip),
        )
        .await;
        (
            StatusCode::OK,
            Json(json!({ "user": { "username": payload.username, "role": "admin" } })),
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
            Some(ip),
        )
        .await;
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        )
    }
}

#[cfg(not(all(unix, feature = "pam-auth")))]
async fn login_local(
    state: &AppState,
    session: &Session,
    payload: &LoginRequest,
    ip: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    let user = match db::users::get_user_by_username(&state.db, &payload.username).await {
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
                Some(ip),
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
            );
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
            Some(ip),
        )
        .await;
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        );
    }

    if let Err(e) = session.cycle_id().await {
        tracing::error!("failed to cycle session id after login: {e}");
    }
    if let Err(e) = session.insert("username", user.username.clone()).await {
        tracing::error!("failed to persist session after login: {e}");
    }
    if let Err(e) = session.insert("role", user.role.clone()).await {
        tracing::error!("failed to persist role in session after login: {e}");
    }
    db::audit::insert(
        &state.db,
        &user.username,
        "auth.login_success",
        None,
        None,
        None,
        Some(ip),
    )
    .await;
    (
        StatusCode::OK,
        Json(json!({
            "user": crate::db::models::UserInfo {
                id: user.id,
                username: user.username.clone(),
                role: user.role,
            }
        })),
    )
}

// ---------------------------------------------------------------------------
// logout
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Logged out successfully"),
    )
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn logout(State(state): State<AppState>, session: Session) -> impl IntoResponse {
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

// ---------------------------------------------------------------------------
// me
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Current authenticated user"),
        (status = 401, description = "Not authenticated"),
    )
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn me(session: Session) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(username)) => {
            #[cfg(all(unix, feature = "pam-auth"))]
            let (role, auth_mode) = ("admin".to_string(), "pam");

            #[cfg(not(all(unix, feature = "pam-auth")))]
            let (role, auth_mode) = (
                session
                    .get::<String>("role")
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "admin".to_string()),
                "local",
            );

            (
                StatusCode::OK,
                Json(
                    json!({ "user": { "username": username, "role": role, "auth_mode": auth_mode } }),
                ),
            )
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Not authenticated"})),
        ),
    }
}

// ---------------------------------------------------------------------------
// update_me
//
// The PAM version takes no arguments (it is a stub that returns 405). The
// local version requires State + Session + Json<UpdateMeRequest>. Unifying
// these signatures would force Axum to extract and parse a JSON body in PAM
// mode even though no body is sent, causing 422 errors before the handler
// runs. Feature-gating at the signature level is intentional here.
// ---------------------------------------------------------------------------

#[cfg(all(unix, feature = "pam-auth"))]
#[utoipa::path(
    patch,
    path = "/api/v1/auth/me",
    tag = "auth",
    security(("cookieAuth" = [])),
    responses(
        (status = 405, description = "Credential changes not supported in PAM auth mode"),
    )
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn update_me() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"error": "credential changes not supported in PAM auth mode"})),
    )
}

#[cfg(not(all(unix, feature = "pam-auth")))]
#[utoipa::path(
    patch,
    path = "/api/v1/auth/me",
    tag = "auth",
    security(("cookieAuth" = [])),
    request_body = UpdateMeRequest,
    responses(
        (status = 200, description = "Credentials updated; session invalidated"),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Not authenticated or wrong current password"),
        (status = 403, description = "Invalid current password"),
        (status = 409, description = "Username already taken"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn update_me(
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

    let user = match db::users::get_user_by_username(&state.db, &current_username).await {
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

#[cfg(test)]
mod client_ip_tests {
    use super::*;

    fn connect_info() -> ConnectInfo<SocketAddr> {
        ConnectInfo("203.0.113.9:1234".parse().unwrap())
    }

    #[test]
    fn ignores_xff_when_not_behind_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.1".parse().unwrap());
        let ip = client_ip(&connect_info(), &headers, false);
        assert_eq!(ip.to_string(), "203.0.113.9");
    }

    #[test]
    fn uses_last_hop_of_xff_when_behind_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "198.51.100.1, 203.0.113.5".parse().unwrap(),
        );
        let ip = client_ip(&connect_info(), &headers, true);
        assert_eq!(ip.to_string(), "203.0.113.5");
    }

    #[test]
    fn falls_back_to_socket_addr_when_behind_proxy_but_no_xff() {
        let headers = HeaderMap::new();
        let ip = client_ip(&connect_info(), &headers, true);
        assert_eq!(ip.to_string(), "203.0.113.9");
    }
}
