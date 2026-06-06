# Spec: Bug Fixes — setup.rs DB masking, race condition, tags silent data loss

## Issues

### B1 — setup.rs `unwrap_or(1)` masks DB errors (MEDIUM)
`status()` line 43 and `create_admin()` line 70 both call:
  `sqlx::query_scalar(...).await.unwrap_or(1)`
If the DB is unavailable, this silently returns 1 (= "setup already done"),
sending a 200 with `needs_setup: false` or a 409, with no log and no 500.
The user is told setup is complete when the server is actually broken.
Fix: match on the Result; log the error and return 500 on Err.

### B2 — setup.rs first-run race condition (MEDIUM)
`create_admin()`: check count → if 0, insert. Two concurrent requests both
see count=0, both attempt INSERT. The DB UNIQUE constraint on `username`
catches the second insert, but the Err branch returns 500 ("Failed to
create user") instead of 409. Fix: inspect the DB error message for
"UNIQUE constraint failed" and return 409 Conflict in that case.

### B3 — services.rs tags serialization silent data loss (LOW)
`create_service()` line 120 and `update_service()` line 249 both use:
  `serde_json::to_string(&t).unwrap_or_default()`
On serialization failure the tags field is silently written as `""` (empty
string) to the DB instead of the original data or an error response.
`Vec<String>` is JSON-serializable in practice, but the fallback violates
the principle of surfacing failures. Fix: match on the Result; return 500
on Err with a tracing::error! log.

## Implementation Notes
All affected functions return `impl IntoResponse` with early-return paths
already typed as `(StatusCode, Json<Value>)` — no return-type changes needed.
The SQLite UNIQUE constraint violation is detectable via
`db_err.message().contains("UNIQUE constraint failed")` on the
`sqlx::Error::Database` variant — reliable for all SQLite versions.

## Files Modified
- `crates/vexboard-server/src/api/setup.rs`
- `crates/vexboard-server/src/api/services.rs`

## Build/Test Commands
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `scripts/preflight.sh`
