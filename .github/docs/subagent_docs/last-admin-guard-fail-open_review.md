# SEC-6 — Last-Admin Guard Fails Open on DB Error — Review

## Summary

Implementation matches spec exactly: both `update_user` and `delete_user`
(crates/vexboard-server/src/api/users.rs) now match on the `Result` from the admin-count query
instead of masking errors with `.unwrap_or(2)`. `Err` now logs and returns 500, `Ok(count) if
count <= 1` still blocks with 409 as before, `Ok(_)` falls through to allow the action — same
observable behavior as before in the success path, fail-closed in the error path.

## Build & Test Results (verbatim)

`cargo fmt --all -- --check` — exit 0, no output (clean).

`cargo clippy --workspace -- -D warnings`:
```
    Checking vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.75s
```
Exit 0, no warnings.

`cargo test -p vexboard-server`:
```
running 34 tests
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
Exit 0. All 34 tests pass, including the existing user-management tests
(`test_create_and_delete_service_as_admin`, `test_admin_route_as_viewer_returns_403`, etc.) —
none exercise this guard directly but all continue to pass, confirming no regression in
surrounding behavior.

`cargo build --release --bin vexboard-server`:
```
    Compiling vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `release` profile [optimized] target(s) in 10.67s
```
Exit 0.

## Review Against Criteria

1. **Specification Compliance** — exact match to spec in both call sites.
2. **Best Practices** — explicit `Result` matching over `unwrap_or` on a security-critical
   guard; consistent with fail-closed principle.
3. **Consistency** — mirrors the existing `Err(e) => { tracing::error!(...); return
   (StatusCode::INTERNAL_SERVER_ERROR, ...) }` pattern used a few lines above each guard for
   the `target` fetch, in the same functions.
4. **Maintainability** — explicit match arms are more readable than the previous silent
   fallback; intent is now visible in the code.
5. **Completeness** — both `update_user` and `delete_user` guards fixed identically; grep
   during Phase 1 confirmed these were the only two occurrences.
6. **Performance** — no impact; same single query, same await point.
7. **Security** — closes a fail-open path that could, under a transient DB error, allow
   removal/demotion of the last admin account, leading to potential permanent lockout.
8. **API Currency** — n/a, no external API involved; sqlx `Result` handling is standard.
9. **Build Validation** — all four approved commands run clean (see above).

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

## Returns

- Build result: PASS (fmt, clippy, tests, release build all clean)
- **PASS**
