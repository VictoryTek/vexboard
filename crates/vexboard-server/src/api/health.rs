use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::AppState;

pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let version = env!("CARGO_PKG_VERSION");

    // Quick DB connectivity check
    let db_ok = sqlx::query("SELECT 1").fetch_one(&state.db).await.is_ok();

    if db_ok {
        (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "version": version
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "degraded",
                "version": version,
                "reason": "database unreachable"
            })),
        )
    }
}
