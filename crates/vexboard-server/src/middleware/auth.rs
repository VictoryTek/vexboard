use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use tower_sessions::Session;

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

pub async fn require_admin(session: Session, request: Request, next: Next) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(_)) => {}
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
                .into_response()
        }
    }
    match session.get::<String>("role").await {
        Ok(Some(ref r)) if r == "admin" => next.run(request).await.into_response(),
        _ => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Admin role required"})),
        )
            .into_response(),
    }
}
