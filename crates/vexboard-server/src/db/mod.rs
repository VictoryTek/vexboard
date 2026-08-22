pub mod audit;
pub mod models;
pub mod users;

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
pub(crate) async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
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

    // Add role column to users (003_user_roles.sql) — idempotent.
    let has_role: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'role'")
            .fetch_one(pool)
            .await?;

    if has_role == 0 {
        let roles_sql = include_str!("migrations/003_user_roles.sql");
        sqlx::raw_sql(roles_sql).execute(pool).await?;
    }

    // Add color column to groups (004_group_color.sql) — idempotent.
    let has_color: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('groups') WHERE name = 'color'")
            .fetch_one(pool)
            .await?;

    if has_color == 0 {
        let color_sql = include_str!("migrations/004_group_color.sql");
        sqlx::raw_sql(color_sql).execute(pool).await?;
    }

    // Dismissed discovery units table (005_dismissed_units.sql) — idempotent (IF NOT EXISTS).
    let dismissed_sql = include_str!("migrations/005_dismissed_units.sql");
    sqlx::raw_sql(dismissed_sql).execute(pool).await?;

    // Add group_id column + quick_link_groups table (006_quick_link_groups.sql) — idempotent.
    let has_group_id: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('quick_links') WHERE name = 'group_id'",
    )
    .fetch_one(pool)
    .await?;

    if has_group_id == 0 {
        let quick_link_groups_sql = include_str!("migrations/006_quick_link_groups.sql");
        sqlx::raw_sql(quick_link_groups_sql).execute(pool).await?;
    }

    // Unique constraint on systemd_unit to prevent duplicate claims (007) — idempotent.
    let unique_unit_sql = include_str!("migrations/007_unique_systemd_unit.sql");
    sqlx::raw_sql(unique_unit_sql).execute(pool).await?;

    // Unify quick_link_groups into groups (008) — idempotent, guarded by quick_link_groups
    // still existing (it's dropped as the final step of the migration).
    let has_quick_link_groups: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'quick_link_groups'",
    )
    .fetch_one(pool)
    .await?;

    if has_quick_link_groups > 0 {
        unify_quick_link_groups(pool).await?;
    }

    // Add skip_tls_verify column to services (009_skip_tls_verify.sql) — idempotent.
    let has_skip_tls_verify: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('services') WHERE name = 'skip_tls_verify'",
    )
    .fetch_one(pool)
    .await?;

    if has_skip_tls_verify == 0 {
        let skip_tls_verify_sql = include_str!("migrations/009_skip_tls_verify.sql");
        sqlx::raw_sql(skip_tls_verify_sql).execute(pool).await?;
    }

    // Notification channels table (010_notification_channels.sql) — idempotent (IF NOT EXISTS).
    let notification_channels_sql = include_str!("migrations/010_notification_channels.sql");
    sqlx::raw_sql(notification_channels_sql)
        .execute(pool)
        .await?;

    // Widen notification_channels.kind to include telegram/gotify (011) — idempotent,
    // guarded by inspecting the table's current CHECK constraint text since SQLite
    // has no direct way to query a constraint's allowed values.
    let notification_channels_table_sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'notification_channels'",
    )
    .fetch_optional(pool)
    .await?;
    let needs_kind_widen = notification_channels_table_sql
        .map(|sql| !sql.contains("'telegram'"))
        .unwrap_or(false);
    if needs_kind_widen {
        let widen_kinds_sql = include_str!("migrations/011_notification_channel_kinds.sql");
        sqlx::raw_sql(widen_kinds_sql).execute(pool).await?;
    }

    tracing::info!("Database migrations applied");
    Ok(())
}

/// One-time migration (008): copy every `quick_link_groups` row into the unified `groups`
/// table (renaming on name collision), remap `quick_links.group_id` to the new ids, rebuild
/// `quick_links` so its FK targets `groups` instead of the removed table, then drop
/// `quick_link_groups`. Runs inside a transaction so a failure partway through leaves the
/// pre-migration schema intact.
async fn unify_quick_link_groups(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    type OldGroupRow = (i64, String, Option<String>, Option<String>, i64);
    let old_groups: Vec<OldGroupRow> = sqlx::query_as(
        "SELECT id, name, icon, color, sort_order FROM quick_link_groups ORDER BY id",
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut id_map: Vec<(i64, i64)> = Vec::with_capacity(old_groups.len());

    for (old_id, name, icon, color, sort_order) in old_groups {
        let mut final_name = name.clone();
        let mut suffix = 2;
        loop {
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM groups WHERE name = ?")
                .bind(&final_name)
                .fetch_one(&mut *tx)
                .await?;
            if exists == 0 {
                break;
            }
            final_name = format!("{name} ({suffix})");
            suffix += 1;
        }

        let new_id: i64 = sqlx::query_scalar(
            "INSERT INTO groups (name, icon, color, sort_order) VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(&final_name)
        .bind(&icon)
        .bind(&color)
        .bind(sort_order)
        .fetch_one(&mut *tx)
        .await?;

        id_map.push((old_id, new_id));
    }

    // Create quick_links_new with group_id referencing groups(id).
    let create_sql = include_str!("migrations/008_unify_groups.sql");
    sqlx::raw_sql(create_sql).execute(&mut *tx).await?;

    // Copy quick_links into quick_links_new, remapping group_id from old
    // quick_link_groups ids to the new groups ids inline via a CASE expression
    // evaluated against the *source* row — this way every value written to
    // quick_links_new.group_id is already a valid groups.id (or NULL), so the
    // new table's FK constraint is never violated during the copy. (Updating
    // quick_links.group_id in place first, before the FK target changes, would
    // fail: the old column still references quick_link_groups(id), and the new
    // ids only exist in groups.)
    let case_expr = if id_map.is_empty() {
        // No quick_link_groups rows existed to remap (the common case for
        // installs that never created one) — every group_id is already NULL.
        "NULL".to_string()
    } else {
        let mut expr = "CASE group_id ".to_string();
        for (old_id, new_id) in &id_map {
            expr.push_str(&format!("WHEN {old_id} THEN {new_id} "));
        }
        expr.push_str("ELSE NULL END");
        expr
    };
    let copy_sql = format!(
        "INSERT INTO quick_links_new (id, title, url, icon, description, sort_order, group_id, created_at, updated_at) \
         SELECT id, title, url, icon, description, sort_order, {case_expr}, created_at, updated_at FROM quick_links;"
    );
    sqlx::raw_sql(&copy_sql).execute(&mut *tx).await?;

    sqlx::raw_sql("DROP TABLE quick_links; ALTER TABLE quick_links_new RENAME TO quick_links;")
        .execute(&mut *tx)
        .await?;

    sqlx::raw_sql("DROP TABLE quick_link_groups;")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Fetch a single value from the `settings` key/value table.
pub async fn get_setting(pool: &SqlitePool, key: &str) -> anyhow::Result<Option<String>> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(value)
}

/// Upsert a value into the `settings` key/value table.
pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically claim a one-time settings flag. Returns `true` if this call performed the
/// insert (the flag was previously unset and is now claimed by `value`), `false` if another
/// caller already claimed it first. Used for the PAM bootstrap-admin grant, where exactly one
/// implicit admin may ever be created.
///
/// Only called from `login_pam` (gated behind the `pam-auth` feature), so it appears unused to
/// a default (non-pam-auth) `cargo clippy` invocation even though it's exercised directly by
/// tests below.
#[allow(dead_code)]
pub async fn try_claim_setting(pool: &SqlitePool, key: &str, value: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await;

    match result {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Who holds the one-time PAM bootstrap-admin claim, relative to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BootstrapAdmin {
    /// This call performed the claim — grant admin and log the one-time warning.
    Granted,
    /// This user already holds the claim from an earlier login — still admin, silently.
    AlreadyHeld,
    /// Someone else claimed it first — this user is a viewer.
    HeldByOther,
}

/// Resolve the PAM bootstrap-admin claim for `username`.
///
/// [`try_claim_setting`] alone is not enough here: it returns `false` as soon as the
/// row exists — *including to the user who claimed it*. Deciding the role from that
/// boolean granted admin on the bootstrap user's **first** login and demoted them to
/// viewer on **every** login after, since the claim they themselves made now made the
/// insert fail. Comparing against the stored holder keeps the grant one-time while
/// remaining idempotent for whoever won it.
///
/// Only called from `login_pam` (gated behind the `pam-auth` feature), so it appears
/// unused to a default `cargo clippy` invocation even though the tests exercise it.
#[allow(dead_code)]
pub async fn claim_bootstrap_admin(
    pool: &SqlitePool,
    username: &str,
) -> anyhow::Result<BootstrapAdmin> {
    if try_claim_setting(pool, "pam_bootstrap_admin", username).await? {
        return Ok(BootstrapAdmin::Granted);
    }
    let holder = get_setting(pool, "pam_bootstrap_admin").await?;
    Ok(if holder.as_deref() == Some(username) {
        BootstrapAdmin::AlreadyHeld
    } else {
        BootstrapAdmin::HeldByOther
    })
}
