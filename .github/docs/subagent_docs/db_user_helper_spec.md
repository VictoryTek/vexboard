# Phase 1 Spec: Shared User Query Helper (db::users)

**Feature:** db_user_helper
**Audit Entry:** 2.3.4
**Date:** 2026-06-06

---

## Current State Analysis

`crates/vexboard-server/src/api/auth.rs` contains two identical inline SQLx query blocks:

**Location 1 — `login_local` (line ~139), inside `#[cfg(not(all(unix, feature = "pam-auth")))]`:**
```rust
let user = sqlx::query_as::<_, crate::db::models::User>(
    "SELECT id, username, password_hash, role, created_at FROM users WHERE username = ?",
)
.bind(&payload.username)
.fetch_optional(&state.db)
.await;
```

**Location 2 — `update_me` (line ~352), inside `#[cfg(not(all(unix, feature = "pam-auth")))]`:**
```rust
let user = match sqlx::query_as::<_, crate::db::models::User>(
    "SELECT id, username, password_hash, role, created_at FROM users WHERE username = ?",
)
.bind(&current_username)
.fetch_optional(&state.db)
.await
{ ... }
```

Both queries are only called in non-PAM auth mode (both callers are already cfg-gated).

`api/users.rs` uses a different query (`SELECT ... WHERE id = ?`, returning `UserPublic`) — it is not part of this extraction.

The `db/` module currently contains: `mod.rs`, `models.rs`, `audit.rs`, and a `migrations/` directory.

---

## Problem Definition

DRY violation: the same query string and result type appear twice. If the `users` table schema changes (e.g., a new column is added to the `SELECT` list, or the `User` struct is updated), both sites must be updated identically. Missing one produces a silent mapping mismatch.

---

## Proposed Solution

Create `crates/vexboard-server/src/db/users.rs` with a single public async helper:

```rust
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
```

The `#[cfg(...)]` guard matches the callers — in PAM mode the function is never called and the compiler elides it, preventing dead-code warnings.

Add `pub mod users;` to `db/mod.rs`.

Replace both inline query blocks in `auth.rs` with `db::users::get_user_by_username(&state.db, ...)`.

---

## Implementation Steps

1. Create `crates/vexboard-server/src/db/users.rs` with the cfg-gated helper function
2. Add `pub mod users;` to `crates/vexboard-server/src/db/mod.rs` (alphabetically after `pub mod models;`)
3. In `crates/vexboard-server/src/api/auth.rs`:
   - In `login_local`: replace the `sqlx::query_as` block with `db::users::get_user_by_username(&state.db, &payload.username).await`
   - In `update_me` (local): replace the `sqlx::query_as` block with `db::users::get_user_by_username(&state.db, &current_username).await`

---

## Dependencies

No new dependencies. All types (`sqlx::SqlitePool`, `crate::db::models::User`) are already in the dependency graph.

Context7 is not required — no new external libraries.

---

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build --release --bin vexboard-server`

All are approved safe commands. No FORBIDDEN COMMANDS involved.

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Call sites miss the new helper and keep the old query | Low | Both sites are in the same file; verified by clippy (dead_code on the helper if not used) |
| PAM-mode compilation failure | None | Helper is cfg-gated identically to its callers |
| Behavior change | None | Mechanical extraction — query string is character-for-character identical |
