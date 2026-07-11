# SEC-6 — Last-Admin Guard Fails Open on DB Error — Spec

## Current State Analysis

Two nearly identical last-admin guards in `crates/vexboard-server/src/api/users.rs`:

- `update_user` (lines 234-238): when demoting a user away from `admin`,
  ```rust
  let admin_count: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
          .fetch_one(&state.db)
          .await
          .unwrap_or(2);
  if admin_count <= 1 { /* block demotion */ }
  ```
- `delete_user` (lines 378-382): identical pattern, guarding deletion of the last admin.

If the `COUNT(*)` query errors (e.g. DB connection issue, lock contention), `unwrap_or(2)`
substitutes a fabricated count of 2, which is `> 1`, so the `admin_count <= 1` check passes and
the guard silently lets the demotion/deletion proceed — even if the target was in fact the only
admin. This is a fail-open pattern on a safety-critical guard: a transient DB hiccup could allow
an admin lockout (no admin accounts left, no way back in without direct DB access).

## Problem Definition

A DB error during the last-admin count check should block the action (fail closed) and surface
a 500, not silently assume "safe to proceed."

## Proposed Solution

Replace `.unwrap_or(2)` with proper `Result` handling: on `Err`, log and return
`500 Internal Server Error` immediately, matching the existing error-handling convention used
elsewhere in both functions (e.g. the `target` fetch's `Err(e) => { tracing::error!(...); return
(StatusCode::INTERNAL_SERVER_ERROR, ...) }` block just above each guard).

## Implementation Steps

1. In `update_user` (crates/vexboard-server/src/api/users.rs:232-245), change:
   ```rust
   if new_role != "admin" && target.role == "admin" {
       let admin_count: i64 =
           sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
               .fetch_one(&state.db)
               .await
               .unwrap_or(2);
       if admin_count <= 1 {
           return (
               StatusCode::CONFLICT,
               Json(json!({"error": "Cannot demote the last admin"})),
           );
       }
   }
   ```
   to check the `Result` explicitly:
   ```rust
   if new_role != "admin" && target.role == "admin" {
       let admin_count: Result<i64, _> =
           sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
               .fetch_one(&state.db)
               .await;
       match admin_count {
           Ok(count) if count <= 1 => {
               return (
                   StatusCode::CONFLICT,
                   Json(json!({"error": "Cannot demote the last admin"})),
               );
           }
           Ok(_) => {}
           Err(e) => {
               tracing::error!("DB error checking admin count: {e}");
               return (
                   StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({"error": "Database error"})),
               );
           }
       }
   }
   ```
2. In `delete_user` (crates/vexboard-server/src/api/users.rs:377-389), apply the identical
   transformation (message text: `"Cannot delete the last admin"`).

## Dependencies

None.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** A transient DB error now blocks the demote/delete action outright rather than
  proceeding.
  **Mitigation:** This is the intended, safe behavior — fail closed on a guard whose only job
  is preventing an unrecoverable admin lockout. The caller sees a 500 and can retry, same as
  every other DB-error path in these two handlers.

## Test Plan

`cargo test -p vexboard-server` — existing tests don't construct a DB-error condition for this
specific query (no test harness for injecting DB failures), so behavior in the `Ok(_)` path
(guard still blocks correctly when count is genuinely 1) is unchanged and covered indirectly by
the fact that no existing test exercises last-admin demotion/deletion at all. No new test added
— matches the project's own fix guidance ("Return 500 on count failure instead") without adding
disproportionate DB-failure-injection infrastructure for this fix.
