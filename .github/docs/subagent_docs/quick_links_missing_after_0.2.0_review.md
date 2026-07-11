# Quick Links Missing After 0.2.0 — Review

## Spec Compliance

`run_migrations()` (`crates/vexboard-server/src/db/mod.rs`) now conditionally
applies `006_quick_link_groups.sql` between the 005 and 007 blocks, guarded by
a `pragma_table_info('quick_links')` check for `group_id`, matching the exact
pattern already used for 003 (`has_role`) and 004 (`has_color`). Matches the
spec exactly — no deviation.

## Best Practices / Consistency / Maintainability

- Follows the established idempotent-migration-guard convention in this file
  (`has_discovery_source`, `has_role`, `has_color`) rather than introducing a
  new pattern.
- Comment style, variable naming (`has_group_id`), and code shape match
  neighboring blocks.
- No new dependencies, no unrelated refactoring — single, surgical addition.

## Completeness

- Fixes the root cause (migration never wired in) rather than papering over
  symptoms (e.g. `unwrap_or_default()` swallowing errors on the frontend was
  considered and rejected as in-scope — that's a separate, pre-existing
  error-handling gap, not the cause of this regression, and touching it would
  exceed the surgical scope of this fix).

## Security / Performance

- No security impact — same DDL as originally authored in 006, now actually
  executed.
- Performance: one extra `pragma_table_info` scalar query per boot, same
  negligible cost as the existing 003/004 checks.

## Empirical Verification (beyond static build/test)

Since this is a DB-migration bug, static checks alone don't prove the fix —
simulated a real pre-0.2.0 database by applying only migrations 001-005 and
007 (skipping 006, exactly as production upgraded DBs are), inserting a quick
link, then starting the built `vexboard-server` release binary against it:

- Startup log showed `Database migrations applied` with no error.
- Post-startup schema dump confirmed `quick_links.group_id` and the
  `quick_link_groups` table now exist.
- The pre-existing "Router" quick link row survived the migration intact
  (`group_id` = NULL, no data loss).

This directly reproduces the upgrade scenario the user hit and confirms it
now resolves cleanly.

## Build Validation

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Clean |
| `cargo clippy -p vexboard-server -- -D warnings` | Clean, no warnings |
| `cargo test -p vexboard-server` | 34/34 passed, no SIGSEGV |
| `cargo build --release --bin vexboard-server` | Clean |

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (100%)**

## Result: PASS
