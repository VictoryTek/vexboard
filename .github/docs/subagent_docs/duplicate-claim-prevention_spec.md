# BUG-8 — Discovery Panel Bypasses Claim Uniqueness Check — Spec

## Current State Analysis

`claim_service` (`crates/vexboard-server/src/api/services.rs:626-654`) is the only endpoint
with a duplicate-claim guard: it runs `SELECT EXISTS(... WHERE systemd_unit = ?)` before
delegating to `create_service`, returning `409 Conflict` if a match is found. But the frontend
discovery panel's "Add" button
(`crates/vexboard-frontend/src/components/discovery_panel.rs:123`) posts straight to
`POST /api/v1/services` (`create_service` directly), which has no such check — nothing stops
two rapid "Add" clicks (or two browser tabs) from creating two service rows for the same
`systemd_unit`. There is also no `UNIQUE` constraint on `services.systemd_unit`
(`crates/vexboard-server/src/db/migrations/001_init.sql:14` — plain nullable `TEXT`), so even
`claim_service`'s own check-then-insert is racy: two concurrent claims can both pass the
`SELECT EXISTS` check before either `INSERT` commits.

`DiscoveredUnit`/`DiscoveredUnitFe` (`crates/vexboard-server/src/discovery/mod.rs:22-31`,
`crates/vexboard-frontend/src/components/discovery_panel.rs:6-12`) has no numeric ID — discovered
units are identified only by `(source, unit_name)`. `claim_service`'s route,
`POST /api/v1/services/{id}/claim`, takes a `Path(id): Path<i64>` that its own doc comment
already calls out as unused ("Discovery unit ID (unused; payload drives insert)"). Rerouting the
frontend to call this endpoint would require synthesizing a meaningless placeholder path segment
for an ID that doesn't semantically exist for a discovered (not-yet-claimed) unit — an awkward
fit. The MASTER_PLAN itself offers a second option: "add the check to the create path."

## Problem Definition

`create_service` (reachable directly by the discovery panel) has no duplicate-`systemd_unit`
guard, and the one guard that does exist (`claim_service`'s pre-check) is racy without a DB-level
constraint backing it.

## Proposed Solution

1. Add a partial `UNIQUE` index on `services.systemd_unit` (excluding `NULL`, since most
   non-discovery services never set this column and SQLite must allow multiple such rows) via a
   new migration. Pre-existing duplicate data (possible today, given the bug) is resolved by
   nulling out `systemd_unit` on all but the earliest (lowest-`id`) claim before the index is
   created, so the migration itself can never fail on dirty existing data.
2. Move the duplicate-check logic out of `claim_service` and into `create_service` itself, so
   every path that creates a service (`POST /services` directly, and `POST
   /services/{id}/claim`, which already delegates to `create_service`) is covered uniformly —
   this is the "add the check to the create path" option, avoiding the awkward ID-less claim
   reroute.
3. Additionally catch a `UNIQUE` constraint violation on the `INSERT` itself (via
   `sqlx::Error::as_database_error().is_some_and(|e| e.is_unique_violation())`, available in the
   pinned `sqlx` 0.8) and map it to `409 Conflict` instead of the generic `500`, so the rare
   race-window case (two requests both pass the pre-check before either commits) still surfaces
   the correct, clean status code rather than an opaque database error — the pre-check alone
   remains racy without this, and the DB constraint from step 1 is the only fully-authoritative
   guard.

No frontend changes are needed — `discovery_panel.rs` continues posting to `POST
/api/v1/services` exactly as it already does; the new guard on `create_service` protects it
transparently.

## Implementation Steps

### 1. New migration: `crates/vexboard-server/src/db/migrations/007_unique_systemd_unit.sql`
```sql
-- 007_unique_systemd_unit.sql
-- Prevent duplicate claims of the same systemd/docker/podman unit.

-- Resolve any pre-existing duplicates (possible before this constraint existed)
-- by clearing systemd_unit on all but the earliest claim, so the index below
-- can always be created cleanly.
UPDATE services
SET systemd_unit = NULL
WHERE systemd_unit IS NOT NULL
  AND id NOT IN (
      SELECT MIN(id) FROM services WHERE systemd_unit IS NOT NULL GROUP BY systemd_unit
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_services_systemd_unit_unique
    ON services(systemd_unit) WHERE systemd_unit IS NOT NULL;
```
Both statements are idempotent (`UPDATE` is a no-op once no duplicates remain; `CREATE UNIQUE
INDEX IF NOT EXISTS` is a no-op if already present), matching the existing unconditional-rerun
pattern already used for migrations `002` and `005` in `run_migrations`.

### 2. `crates/vexboard-server/src/db/mod.rs`

In `run_migrations`, after the existing `005_dismissed_units.sql` block, add:
```rust
// Unique constraint on systemd_unit to prevent duplicate claims (007) — idempotent.
let unique_unit_sql = include_str!("migrations/007_unique_systemd_unit.sql");
sqlx::raw_sql(unique_unit_sql).execute(pool).await?;
```

### 3. `crates/vexboard-server/src/api/services.rs` — `create_service`

At the top of `create_service`, before the existing `tags_json` handling, add the duplicate
check (moved from `claim_service`):
```rust
if let Some(ref unit) = payload.systemd_unit {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM services WHERE systemd_unit = ? LIMIT 1)",
    )
    .bind(unit)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if exists {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "Unit already claimed"})),
        );
    }
}
```
And change the `Err(e)` arm of the final `INSERT` match to distinguish a unique-constraint
violation:
```rust
Err(e) => {
    if e.as_database_error().is_some_and(|de| de.is_unique_violation()) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "Unit already claimed"})),
        );
    }
    tracing::error!("Failed to create service: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "Failed to create service"})),
    )
}
```

### 4. `crates/vexboard-server/src/api/services.rs` — `claim_service`

Remove the now-duplicated pre-check block (lines ~632-648); `claim_service` becomes a thin
wrapper that just delegates to `create_service`, which now owns this guard uniformly:
```rust
pub(crate) async fn claim_service(
    State(state): State<AppState>,
    session: Session,
    Path(_id): Path<i64>,
    Json(payload): Json<CreateService>,
) -> axum::response::Response {
    // Reuse create logic (dedup + audit entry both handled there).
    create_service(State(state), session, Json(payload))
        .await
        .into_response()
}
```
(`Path(id)` renamed to `Path(_id)` since it was already unused and remains so — keeps the route
signature/shape unchanged since the route itself, `/{id}/claim`, is unaffected by this fix.)

## Dependencies

None new — `sqlx::error::DatabaseError::is_unique_violation()` is already available in the
pinned `sqlx` 0.8 (confirmed present in `sqlx-core-0.8.6/src/error.rs:252`).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** The migration's `UPDATE ... SET systemd_unit = NULL` on existing duplicate rows
  changes data — a previously "duplicate-claimed" service loses its D-Bus/container tracking
  link (falls back to no probing via `systemd_unit`, though `url`-based probing, if configured,
  is unaffected).
  **Mitigation:** This is a deliberate, minimal-impact resolution of already-invalid state (two
  rows can't both validly own the same unit); the row itself is preserved, not deleted, and only
  the ambiguous duplicate claim is cleared, keeping the *first* (lowest-id, i.e. originally
  created) claim intact.
- **Risk:** Removing the pre-check from `claim_service` and relying on `create_service`'s copy
  changes nothing observably — same 409 response, same audit trail (still written by
  `create_service` on success) — but is worth calling out as intentional consolidation, not a
  regression in `claim_service`'s behavior.
- **Risk:** The pre-check (`SELECT EXISTS`) is still inherently racy on its own.
  **Mitigation:** This is precisely why the DB-level `UNIQUE` index (step 1) plus the
  `is_unique_violation()` catch (step 3) exist — the pre-check is a fast-path UX nicety (avoids
  a wasted `INSERT` attempt in the common case), while the index is what actually guarantees no
  duplicate ever persists, and the `is_unique_violation()` mapping keeps even the race-window
  response code clean (409, not 500).

## Test Plan

`cargo test -p vexboard-server` — existing tests `test_create_service_as_admin` and
`test_create_and_delete_service_as_admin` continue to exercise `create_service` on services
without a `systemd_unit` (unaffected by the new guard, which only activates when
`payload.systemd_unit` is `Some`). No new test is added: reproducing the race condition
deterministically would require controlling SQLite transaction timing, which the existing test
harness (`crate::tests`, in-memory pool, no fixture for concurrent-request simulation) doesn't
support; the fix's correctness is verified by `cargo build`/`clippy` compiling the new
`is_unique_violation()` branch and pre-check logic, plus manual reasoning: the `UNIQUE INDEX`
guarantees correctness at the DB layer regardless of any application-level race, which is the
actual backstop this fix is designed to close.
