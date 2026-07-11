# SEC-7 — Audit Log Exposed to Viewer Role — Review

## Summary

Implementation matches spec exactly: `.nest("/api/v1/audit", audit::router())` moved from
`viewer_protected` (gated by `require_auth`, any authenticated session) to `admin_protected`
(gated by `require_admin`) in `crates/vexboard-server/src/api/mod.rs`. `audit::router()` itself
is unchanged — the fix is purely a router-composition move, as intended (audit.rs already
exposes a single unsplit router, correct for admin-only nesting).

## Build & Test Results (verbatim)

`cargo fmt --all -- --check` — exit 0, no output (clean).

`cargo clippy --workspace -- -D warnings`:
```
    Checking vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.63s
```
Exit 0, no warnings.

`cargo test -p vexboard-server`:
```
running 34 tests
...
test tests::test_admin_route_as_viewer_returns_403 ... ok
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
Exit 0. `test_admin_route_as_viewer_returns_403` (validates the `require_admin` middleware
mechanism generally) continues to pass, confirming the admin-gating layer this fix now relies
on for `/api/v1/audit` is functioning correctly elsewhere in the router.

`cargo build --release --bin vexboard-server`:
```
    Compiling vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `release` profile [optimized] target(s) in 10.50s
```
Exit 0.

## Review Against Criteria

1. **Specification Compliance** — exact match to spec.
2. **Best Practices** — reuses the existing, already-proven `require_admin` middleware layer
   rather than introducing new access-control logic.
3. **Consistency** — `/api/v1/audit` now sits alongside `users`, `settings`, and `discovery` in
   `admin_protected`, matching its sensitivity level.
4. **Maintainability** — single-nest relocation; router composition remains easy to scan.
5. **Completeness** — fully resolves A-A8; no other route references `audit::router()`.
6. **Performance** — no impact.
7. **Security** — closes the exposure: viewers can no longer enumerate usernames, watch admin
   activity, or see client IPs via the audit log.
8. **API Currency** — n/a, purely internal router composition.
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
