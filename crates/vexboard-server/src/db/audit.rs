use sqlx::SqlitePool;

/// Write an audit record. Errors are logged but never propagated — audit failures
/// must never fail the user-facing operation.
pub async fn insert(
    pool: &SqlitePool,
    actor: &str,
    action: &str,
    resource_type: Option<&str>,
    resource_id: Option<i64>,
    detail: Option<&str>,
    ip_addr: Option<&str>,
) {
    let result = sqlx::query(
        "INSERT INTO audit_log (actor, action, resource_type, resource_id, detail, ip_addr) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(actor)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(detail)
    .bind(ip_addr)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!("audit log write failed (action={action}): {e}");
    }
}
