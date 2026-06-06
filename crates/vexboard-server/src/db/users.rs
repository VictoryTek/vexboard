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
