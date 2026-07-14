use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use tower_sessions::Session;

use crate::db;
use crate::AppState;

/// Resolve the effective role for the logged-in `username`.
///
/// The `users` row is the source of truth. A session only ever caches the role
/// at login, so a session that carries a `username` but no `role` — one created
/// before roles existed, or whose role write failed — silently downgraded a real
/// admin to a viewer, with no way back except re-login. Reading the role from the
/// database on each request makes that self-healing and lets an admin's role
/// change take effect immediately instead of at their next login.
///
/// PAM users have no `users` row (their role is derived from config at login), so
/// the session-cached role remains the fallback for that mode.
pub async fn resolve_role(state: &AppState, session: &Session, username: &str) -> String {
    if let Ok(Some(user)) = db::users::get_user_by_username(&state.db, username).await {
        return user.role;
    }
    session
        .get::<String>("role")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "viewer".to_string())
}

pub async fn require_auth(session: Session, request: Request, next: Next) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(_)) => next.run(request).await.into_response(),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Not authenticated"})),
        )
            .into_response(),
    }
}

pub async fn require_admin(
    State(state): State<AppState>,
    session: Session,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    let username = match session.get::<String>("username").await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
                .into_response()
        }
    };
    if resolve_role(&state, &session, &username).await == "admin" {
        next.run(request).await.into_response()
    } else {
        (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Admin role required"})),
        )
            .into_response()
    }
}
