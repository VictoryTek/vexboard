# SEC-5 — `/auth/me` Defaults Missing Role to "admin" — Review

## Summary

Implementation matches spec exactly: the `unwrap_or_else` fallback in the non-PAM branch of
`me()` (crates/vexboard-server/src/api/auth.rs:283) changed from `"admin"` to `"viewer"`. Grep
confirmed this is the only occurrence of the fail-open pattern in the server crate. One-line,
surgical change with no other code paths touched.

## Build & Test Results (verbatim)

`cargo fmt --all -- --check` — exit 0, no output (clean).

`cargo clippy --workspace -- -D warnings`:
```
    Checking vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.62s
```
Exit 0, no warnings.

`cargo test -p vexboard-server`:
```
running 34 tests
...
test tests::test_me_authenticated_returns_username_and_role ... ok
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
Exit 0.

`cargo build --release --bin vexboard-server`:
```
    Compiling vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `release` profile [optimized] target(s) in 10.64s
```
Exit 0.

## Review Against Criteria

1. **Specification Compliance** — exact match to spec.
2. **Best Practices** — fail-closed default for privilege determination; standard least-
   privilege practice.
3. **Consistency** — no other fallback pattern in the codebase to align with; this was the sole
   outlier.
4. **Maintainability** — trivial, self-evident one-line change.
5. **Completeness** — fully resolves B-M3; grep confirms no other instance of this bug pattern.
6. **Performance** — no impact.
7. **Security** — removes a privilege-escalation-by-omission path: a session missing its role
   key no longer silently grants admin.
8. **API Currency** — n/a, no external API involved.
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
