# BUG-7 — Expired Sessions Never Deleted — Spec

## Current State Analysis

`SqliteSessionStore::load` (`crates/vexboard-server/src/session_store.rs:90-128`) checks
`expiry_date <= OffsetDateTime::now_utc()` and returns `Ok(None)` for expired rows, but never
issues a `DELETE` — the expired row stays in `tower_sessions` forever. There is no periodic
cleanup task anywhere in the codebase (verified: no other reference to `tower_sessions` table
maintenance outside `session_store.rs`). Since `save()` uses `INSERT OR REPLACE` keyed by
session ID, a session that's never explicitly deleted (e.g. `logout`, or the new
`delete_by_username` from SEC-1) accumulates one permanent dead row per login once it expires.
With the default 7-day TTL (`session_ttl_hours = 168`, `config/default.toml`), a busy or
long-running deployment accumulates unbounded dead rows over time.

Investigated `tower-sessions` 0.15.0's built-in solution: `session_store::ExpiredDeletion`
(re-exported from `tower_sessions::session_store`) defines a required `delete_expired(&self)`
method and a default `continuously_delete_expired(self, period)` driver loop. Checked the pinned
crate source directly (`~/.cargo/registry/.../tower-sessions-core-0.15.0/src/session_store.rs`):
the `continuously_delete_expired` default method is gated behind the `deletion-task` Cargo
feature on `tower-sessions-core`, which the top-level `tower-sessions` crate does **not**
forward as a feature of its own (its `Cargo.toml` only enables `deletion-task` in
`[dev-dependencies]`, not for downstream consumers) — using it would require adding a new,
separate direct dependency on `tower-sessions-core` with an explicit feature override just to
reach one driver method. The `ExpiredDeletion` trait itself and its required `delete_expired`
method, however, are **not** feature-gated and are already reachable via the existing
`tower-sessions` dependency with no Cargo.toml changes.

## Problem Definition

No mechanism ever removes expired session rows, causing unbounded `tower_sessions` table growth.

## Proposed Solution

1. Implement `tower_sessions::session_store::ExpiredDeletion` for `SqliteSessionStore`, providing
   `delete_expired()` as a single `DELETE FROM tower_sessions WHERE expiry_date <= ?` query —
   no new dependency needed.
2. Drive it with a small hand-rolled periodic loop (`session_cleanup_loop`), matching this
   codebase's existing convention for every other background task (`probe::start_probe_loop`,
   `discovery::systemd::discovery_loop`, `discovery::docker::docker_discovery_loop`,
   `metrics::system::metrics_loop`) — a `loop { do_work().await; sleep(interval).await; }` shape
   — rather than pulling in `tower-sessions-core` directly just for `continuously_delete_expired`.
   This keeps the fix dependency-free and consistent with the rest of the project instead of
   introducing a second, differently-sourced background-task pattern.
3. Spawn the loop once at startup from `main.rs`, alongside the other background tasks, on a
   1-hour interval (frequent enough to bound growth well within the 7-day default TTL, cheap
   enough — one indexed-by-primary-key-range delete — to not matter at that cadence).

## Implementation Steps

### 1. `crates/vexboard-server/src/session_store.rs`

Add the trait implementation and cleanup loop:
```rust
#[async_trait]
impl tower_sessions::session_store::ExpiredDeletion for SqliteSessionStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        sqlx::query("DELETE FROM tower_sessions WHERE expiry_date <= ?")
            .bind(OffsetDateTime::now_utc().unix_timestamp())
            .execute(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}

/// Periodically deletes expired session rows so the `tower_sessions` table
/// doesn't grow unbounded — `load()` only filters expired rows out of query
/// results, it never removes them.
pub async fn session_cleanup_loop(store: SqliteSessionStore, interval: std::time::Duration) {
    use tower_sessions::session_store::ExpiredDeletion;
    loop {
        if let Err(e) = store.delete_expired().await {
            tracing::warn!("failed to delete expired sessions: {e}");
        }
        tokio::time::sleep(interval).await;
    }
}
```
(`ExpiredDeletion` needs to be in scope for `store.delete_expired()` to resolve; imported locally
inside the function to avoid broadening the file's top-level `use tower_sessions::{...}` import
for a name only needed here.)

### 2. `crates/vexboard-server/src/main.rs`

Alongside the other background-task spawns (near the discovery/probe/metrics loops, after
`session_store.migrate().await?`), add:
```rust
let cleanup_store = session_store.clone();
tokio::spawn(async move {
    session_store::session_cleanup_loop(cleanup_store, std::time::Duration::from_secs(3600))
        .await;
});
```

## Dependencies

None new — `tower_sessions::session_store::ExpiredDeletion` is already reachable through the
existing `tower-sessions` dependency; no Cargo.toml change, no feature flag change.

## Configuration Changes

None. The 1-hour cleanup interval is a fixed constant, matching how `probe::mod.rs`'s scheduler
tick was recently fixed with a fixed constant (`TICK_SECS`) rather than adding new config
surface for an internal implementation detail.

## Risks and Mitigations

- **Risk:** `SqliteSessionStore` is cloned into the spawned task — verify it's cheap to clone.
  **Mitigation:** `SqliteSessionStore` derives `Clone` and wraps only a `SqlitePool`
  (`crates/vexboard-server/src/session_store.rs:15-18`), itself cheap to clone (internally
  pooled/reference-counted), matching how `db.clone()` and `session_store.clone()` are already
  passed into other spawned tasks in `main.rs`.
- **Risk:** Deleting rows while `load()`/`save()` are concurrently in flight.
  **Mitigation:** SQLite handles concurrent DML safely at the connection-pool level (already
  relied upon throughout this codebase for concurrent reads/writes); a `DELETE ... WHERE
  expiry_date <= ?` only ever removes rows that are already treated as nonexistent by `load()`'s
  own expiry check, so there's no risk of deleting a session another request considers live.

## Test Plan

`cargo test -p vexboard-server` — no existing test exercises session expiry/cleanup timing
(would require constructing rows with controlled `expiry_date` values and asserting deletion,
which the current test harness has no fixture for). No new test added: this is a straightforward
periodic-maintenance query mirroring the exact shape of the project's other untested background
loops (probe/discovery/metrics), verified via `cargo build`/`clippy` compiling the new trait impl
and loop function, plus the existing `test_logout_invalidates_session` test continuing to pass
unaffected (logout's existing `session.flush()` path is untouched by this addition).
