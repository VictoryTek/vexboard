use std::str::FromStr;

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;
use tower_sessions::{
    session::{Id, Record},
    session_store, SessionStore,
};

/// SQLite-backed session store that survives server restarts.
///
/// Sessions are stored in the `tower_sessions` table within the application's
/// existing SQLite database. Call `migrate()` once at startup before use.
#[derive(Clone, Debug)]
pub struct SqliteSessionStore {
    pool: SqlitePool,
}

impl SqliteSessionStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create the `tower_sessions` table if it does not already exist.
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tower_sessions (
                id          TEXT PRIMARY KEY NOT NULL,
                data        TEXT NOT NULL,
                expiry_date INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete every live session belonging to `username`. Used to revoke access
    /// immediately after an admin changes a user's role/username or deletes them.
    pub async fn delete_by_username(&self, username: &str) -> Result<(), sqlx::Error> {
        let rows = sqlx::query("SELECT id, data FROM tower_sessions")
            .fetch_all(&self.pool)
            .await?;

        for row in rows {
            let id: String = row.try_get("id")?;
            let data: String = row.try_get("data")?;
            let matches = serde_json::from_str::<serde_json::Value>(&data)
                .ok()
                .and_then(|v| {
                    v.get("username")
                        .and_then(|u| u.as_str().map(str::to_string))
                })
                .is_some_and(|u| u == username);

            if matches {
                sqlx::query("DELETE FROM tower_sessions WHERE id = ?")
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let id = record.id.to_string();
        let data = serde_json::to_string(&record.data)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;
        let expiry = record.expiry_date.unix_timestamp();

        sqlx::query(
            "INSERT OR REPLACE INTO tower_sessions (id, data, expiry_date) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(data)
        .bind(expiry)
        .execute(&self.pool)
        .await
        .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let id = session_id.to_string();
        let row = sqlx::query("SELECT id, data, expiry_date FROM tower_sessions WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let expiry_ts: i64 = row
            .try_get("expiry_date")
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;
        let expiry_date = OffsetDateTime::from_unix_timestamp(expiry_ts)
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;

        if expiry_date <= OffsetDateTime::now_utc() {
            return Ok(None);
        }

        let raw_id: String = row
            .try_get("id")
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;
        let raw_data: String = row
            .try_get("data")
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;

        let id = Id::from_str(&raw_id).map_err(|e| session_store::Error::Decode(e.to_string()))?;
        let data = serde_json::from_str(&raw_data)
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;

        Ok(Some(Record {
            id,
            data,
            expiry_date,
        }))
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let id = session_id.to_string();
        sqlx::query("DELETE FROM tower_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}
