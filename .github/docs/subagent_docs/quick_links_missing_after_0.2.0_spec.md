# Quick Links Missing After Upgrading to 0.2.0 — Spec

## Current State Analysis

`crates/vexboard-server/src/db/mod.rs::run_migrations()` (lines 34-88) executes
migrations in this order: `001_init.sql`, `002_audit_log.sql`, an inline
`discovery_source` backfill, conditionally `003_user_roles.sql`, conditionally
`004_group_color.sql`, `005_dismissed_units.sql`, then jumps straight to
`007_unique_systemd_unit.sql`.

`crates/vexboard-server/src/db/migrations/006_quick_link_groups.sql` exists on
disk (added in commit `fdd50ce`, "feat(quick-links): add group and
drag-to-reorder support for quick links") but is never `include_str!`'d or
executed anywhere in `run_migrations()`. It was never wired in.

Migration 006 does:
```sql
CREATE TABLE IF NOT EXISTS quick_link_groups (...);
ALTER TABLE quick_links ADD COLUMN group_id INTEGER REFERENCES quick_link_groups(id) ON DELETE SET NULL;
```

On a database created before 0.2.0 (i.e. any upgraded device), the
`quick_links` table has no `group_id` column and `quick_link_groups` never
gets created.

`crates/vexboard-server/src/api/quick_links.rs:41-44` (`list_quick_links`)
queries:
```sql
SELECT id, title, url, icon, description, group_id, sort_order FROM quick_links ...
```
Against an un-migrated DB this fails with `no such column: group_id`, hits
the `Err` branch (lines 48-51) and returns `500 INTERNAL_SERVER_ERROR`.

The frontend (`crates/vexboard-frontend/src/pages/dashboard/mod.rs:106-107`)
calls `fetch_quick_links().await.unwrap_or_default()`, silently swallowing
the 500 into an empty `Vec`. `quick_links_section.rs:69-72` then renders
nothing when the list is empty (`EitherOf3::A(())`), with no error surfaced
to the user — exactly matching the reported symptom.

## Problem Definition

Migration `006_quick_link_groups.sql` is never executed by `run_migrations()`,
so pre-0.2.0 databases never gain the `group_id` column or
`quick_link_groups` table that 0.2.0's quick-links API code now assumes
exists. Any device upgraded from an older version (rather than a fresh
install) hits this.

## Proposed Solution

Wire migration 006 into `run_migrations()`, following the same idempotent
conditional pattern already used for 003/004 (check
`pragma_table_info` before applying, since 006 contains a non-idempotent
`ALTER TABLE ... ADD COLUMN` that will error if run twice).

## Implementation Steps

1. In `run_migrations()` (`crates/vexboard-server/src/db/mod.rs`), after the
   `005_dismissed_units.sql` block (line 80) and before
   `007_unique_systemd_unit.sql` (line 83), add:
   ```rust
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
   ```
2. No other files require changes — the migration SQL itself is already
   correct; it just needs to run.

## Dependencies

None — reuses existing `sqlx::query_scalar` / `sqlx::raw_sql` / `include_str!`
patterns already present in the same function.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Running the check on every boot adds one extra `pragma_table_info`
  query. **Mitigation:** Negligible — same cost as the existing 003/004
  checks that already run on every boot.
- **Risk:** A database that already has `group_id` (fresh 0.2.0 install, or
  a device where 006 was somehow already applied) must not re-run the
  `ALTER TABLE`. **Mitigation:** The `has_group_id == 0` guard prevents
  double-application, matching the existing idempotent pattern for 003/004.
- **Risk:** `CREATE TABLE IF NOT EXISTS quick_link_groups` inside the same
  guarded block only runs when `group_id` is absent — if a hand-repaired DB
  somehow has `group_id` but not `quick_link_groups`, this fix won't create
  the table. **Mitigation:** Not a realistic state for any existing 0.1.x
  database (both were only ever added together in 006); out of scope for a
  surgical fix.

## Approved Validation Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy -p vexboard-server -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`
