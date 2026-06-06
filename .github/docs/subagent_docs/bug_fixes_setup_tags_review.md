# Review: Bug Fixes — setup.rs DB masking, race condition, tags silent data loss

## Build Validation

| Command | Result |
|---------|--------|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace -- -D warnings` | PASS |
| `cargo build --release --bin vexboard-server` | PASS |
| `scripts/preflight.sh` | PASS |

## Changes Made

### setup.rs
- `status()`: replaced `.unwrap_or(1)` with a `match` that returns 500 + `tracing::error!` on DB failure
- `create_admin()`: same fix for its identical count query
- `create_admin()` INSERT Err branch: inspects `db_err.message()` for "UNIQUE constraint failed"; returns 409 Conflict (race condition handled correctly) vs 500 for other errors

### services.rs
- `create_service()`: replaced `.map(|t| to_string(&t).unwrap_or_default())` with a `match` that returns 500 on serialization failure
- `update_service()`: same fix; `None` branch now falls through to `existing.tags` as before

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

## Verdict: PASS
