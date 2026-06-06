use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::db::models::AuditEvent;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_audit))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(crate) struct AuditQuery {
    /// Maximum number of entries to return (1–500, default 50).
    #[serde(default = "default_limit")]
    limit: i64,
    /// Pagination offset (default 0).
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[utoipa::path(
    get,
    path = "/api/v1/audit",
    tag = "audit",
    security(("cookieAuth" = [])),
    params(AuditQuery),
    responses(
        (status = 200, description = "Paginated audit log entries"),
        (status = 401, description = "Not authenticated"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_audit(
    State(state): State<AppState>,
    Query(params): Query<AuditQuery>,
) -> impl IntoResponse {
    let limit = params.limit.clamp(1, 500);
    let offset = params.offset.max(0);

    let total: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&state.db)
        .await
    {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("Failed to count audit log: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch audit log"})),
            );
        }
    };

    let entries = match sqlx::query_as::<_, AuditEvent>(
        "SELECT id, actor, action, resource_type, resource_id, detail, ip_addr, created_at \
         FROM audit_log ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to fetch audit log: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch audit log"})),
            );
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "entries": entries,
            "total": total,
            "limit": limit,
            "offset": offset,
        })),
    )
}
