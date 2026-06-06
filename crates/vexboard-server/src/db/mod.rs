pub mod audit;
pub mod models;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

/// Initialize the SQLite connection pool and run migrations.
#[tracing::instrument(skip_all)]
pub async fn init_pool(db_path: &Path) -> anyhow::Result<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let options = SqliteConnectOptions::from_str(&db_url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    run_migrations(&pool).await?;

    Ok(pool)
}

/// Run embedded SQL migrations.
async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    // Run the init migration manually since we embed it
    let init_sql = include_str!("migrations/001_init.sql");
    sqlx::raw_sql(init_sql).execute(pool).await?;

    // Audit log table (idempotent — uses IF NOT EXISTS)
    let audit_sql = include_str!("migrations/002_audit_log.sql");
    sqlx::raw_sql(audit_sql).execute(pool).await?;

    // Backfill for existing databases created before discovery_source existed.
    let has_discovery_source: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('services') WHERE name = 'discovery_source'",
    )
    .fetch_one(pool)
    .await?;

    if has_discovery_source == 0 {
        sqlx::query("ALTER TABLE services ADD COLUMN discovery_source TEXT")
            .execute(pool)
            .await?;
    }

    tracing::info!("Database migrations applied");
    Ok(())
}
