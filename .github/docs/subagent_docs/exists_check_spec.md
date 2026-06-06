# Phase 1 Spec: Replace COUNT(*) with EXISTS for Duplicate Detection

**Feature:** exists_check
**Audit Entry:** 2.3.5
**Date:** 2026-06-06

---

## Current State Analysis

Three locations use `SELECT COUNT(*) FROM ...` purely to test for the existence of at least one
matching row. In each case the count value itself is never used — only whether it is zero or
non-zero matters.

### Location 1 — `discovery/systemd.rs:97–106`

```rust
let claimed =
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM services WHERE systemd_unit = ?")
        .bind(name)
        .fetch_one(db)
        .await
        .unwrap_or(0);

if claimed > 0 {
    continue;
}
```

### Location 2 — `discovery/docker.rs:116–127`

```rust
let claimed = sqlx::query_scalar::<_, i64>(
    "SELECT COUNT(*) FROM services WHERE display_name = ? OR systemd_unit = ?",
)
.bind(&name)
.bind(&name)
.fetch_one(db)
.await
.unwrap_or(0);

if claimed > 0 {
    continue;
}
```

### Location 3 — `api/services.rs:473–487` (`claim_service`)

Audit cited line 234–238 but that number is stale from pre-refactor; the check is now in
`claim_service`:

```rust
let exists =
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM services WHERE systemd_unit = ?")
        .bind(unit)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

if exists > 0 {
    return (...).into_response();
}
```

---

## Problem Definition

`COUNT(*)` without a `LIMIT` scans **all** matching rows to produce the count. When only
existence is needed, `EXISTS(SELECT 1 FROM ... LIMIT 1)` short-circuits on the first match.
With an index on the checked column the difference is negligible, but `EXISTS` is semantically
correct — it names the intent (does at least one row match?) rather than a quantity.

---

## Proposed Solution

Replace all three occurrences with `SELECT EXISTS(SELECT 1 FROM ... LIMIT 1)`, returning `bool`.
SQLite's `EXISTS(...)` returns the integer `0` or `1`, which SQLx maps cleanly to `bool` via
`query_scalar::<_, bool>`. The condition simplifies from `count > 0` to a direct boolean test.

### Replacement pattern

```rust
// Before
let claimed = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM services WHERE col = ?")
    .bind(val)
    .fetch_one(db)
    .await
    .unwrap_or(0);
if claimed > 0 { ... }

// After
let claimed = sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM services WHERE col = ? LIMIT 1)",
)
.bind(val)
.fetch_one(db)
.await
.unwrap_or(false);
if claimed { ... }
```

---

## Implementation Steps

1. `discovery/systemd.rs`: replace `COUNT(*)` block with `EXISTS` returning `bool`
2. `discovery/docker.rs`: replace `COUNT(*)` block with `EXISTS` returning `bool`
3. `api/services.rs` (`claim_service`): replace `COUNT(*)` block with `EXISTS` returning `bool`

---

## Dependencies

No new dependencies. All changes are query-string substitutions within existing `sqlx`
`query_scalar` call sites.

Context7 not required — no new external libraries.

---

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `bash scripts/preflight.sh`

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| `bool` mapping differs across SQLite versions | None | SQLite `EXISTS(...)` always returns 0 or 1; sqlx maps integer 0/1 to bool correctly |
| Behavior change | None | Semantically identical — both paths continue/return on at least one matching row |
| LIMIT 1 inside EXISTS redundant | Low | Included for clarity; SQLite optimiser already short-circuits EXISTS, LIMIT 1 is a no-op but makes intent explicit |
