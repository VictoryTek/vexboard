#[cfg(not(all(unix, feature = "pam-auth")))]
pub async fn get_user_by_username(
    pool: &sqlx::SqlitePool,
    username: &str,
) -> Result<Option<crate::db::models::User>, sqlx::Error> {
    sqlx::query_as::<_, crate::db::models::User>(
        "SELECT id, username, password_hash, role, created_at FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

/// Returns the single account on the instance, or `None` if there are zero or
/// more than one (ambiguous — no way to guess which human is at the keyboard).
#[cfg(not(all(unix, feature = "pam-auth")))]
pub async fn get_sole_user(
    pool: &sqlx::SqlitePool,
) -> Result<Option<crate::db::models::User>, sqlx::Error> {
    let mut rows = sqlx::query_as::<_, crate::db::models::User>(
        "SELECT id, username, password_hash, role, created_at FROM users LIMIT 2",
    )
    .fetch_all(pool)
    .await?;
    if rows.len() == 1 {
        Ok(Some(rows.remove(0)))
    } else {
        Ok(None)
    }
}
