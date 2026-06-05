# SQLite-Backed Session Store — Specification
**Feature:** Persistent session store (audit item 2.2.1)
**Date:** 2026-06-05
**Phase:** 1 — Research & Specification

---

## Current State

`main.rs:101` uses `tower_sessions::MemoryStore::default()` as the session backend.
This means every server restart (update, crash, systemd reload) invalidates all active sessions,
forcing every logged-in user to re-authenticate.

The `AppState` already holds a `SqlitePool` (`db`), and the database file persists across restarts —
making SQLite the natural session backend with zero additional infrastructure.

---

## Problem Definition

- Sessions are lost on any server restart.
- In-memory store scales only to a single process; no durability.
- The `main.rs:102` TODO comment explicitly flags this gap.

---

## Proposed Solution

Replace `MemoryStore` with `tower_sessions_sqlx_store::SqliteStore`, passing the existing pool.
`SqliteStore::migrate()` creates a `tower_sessions` table in the same SQLite database on startup.

No new database file, no new infrastructure, no schema migration file needed — the store manages
its own table via `migrate()`.

---

## Dependencies

| Crate | Version | Justification |
|---|---|---|
| `tower-sessions-sqlx-store` | `0.15` | Matches `tower-sessions = "0.15"` in workspace; SQLite backend |

`sqlx` with the `sqlite` feature is already in the workspace — no additional features needed.

The crate is a server-only concern; added directly to `crates/vexboard-server/Cargo.toml`,
not to workspace dependencies.

---

## Implementation Steps

1. Add `tower-sessions-sqlx-store = { version = "0.15", features = ["sqlite"] }` to
   `crates/vexboard-server/Cargo.toml` `[dependencies]`.
2. In `crates/vexboard-server/src/main.rs`:
   - Remove `use tower_sessions::MemoryStore;` import.
   - Add `use tower_sessions_sqlx_store::SqliteStore;` import.
   - Replace `let session_store = MemoryStore::default();` with:
     ```rust
     let session_store = SqliteStore::new(db.clone());
     session_store.migrate().await?;
     ```
3. No changes to `AppState`, config, migrations, or any handler.

---

## Build / Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
  - Note: runs only server crate on native target; frontend excluded by FORBIDDEN COMMANDS rule.
- `cargo build --release --bin vexboard-server`

---

## Risks & Mitigations

| Risk | Mitigation |
|---|---|
| `tower-sessions-sqlx-store 0.15` version mismatch with `tower-sessions 0.15` | Both are in the `0.15` series — confirmed compatible; Cargo will error on trait mismatch at compile time |
| `SqliteStore::migrate()` fails if the database is read-only | Pool is already opened with `mode=rwc` and WAL mode — write access is guaranteed |
| Session table conflicts with existing schema | `migrate()` uses `CREATE TABLE IF NOT EXISTS` — idempotent |
