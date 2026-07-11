# SEC-4 — Login Rate Limiter Boot-Time Panic — Review

## Summary

Implementation matches spec exactly: `now - self.window` (panicking `Instant` subtraction)
replaced with `now.checked_sub(self.window)`, and the pruning loop is now gated on
`Some(cutoff)`. When the process hasn't been alive as long as `window`, no attempts are pruned
for that call — correct, since nothing recorded so far could be older than `window` yet. The
allow/deny counting logic below is untouched.

## Build & Test Results (verbatim)

`cargo fmt --all -- --check` — exit 0, no output (clean).

`cargo clippy --workspace -- -D warnings`:
```
    Checking vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.59s
```
Exit 0, no warnings.

`cargo test -p vexboard-server`:
```
running 34 tests
...
test rate_limit::tests::distinct_ips_have_independent_budgets ... ok
test rate_limit::tests::blocks_after_max_attempts_within_window ... ok
test rate_limit::tests::rate_limited_call_with_no_prior_attempts_prunes_empty_entry ... ok
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
Exit 0. All three existing `rate_limit` unit tests still pass unchanged.

`cargo build --release --bin vexboard-server`:
```
    Compiling vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `release` profile [optimized] target(s) in 10.64s
```
Exit 0.

## Review Against Criteria

1. **Specification Compliance** — exact match to the spec's proposed diff.
2. **Best Practices** — uses `Instant::checked_sub`, the idiomatic non-panicking alternative,
   exactly as CLAUDE.md's own MASTER_PLAN fix note prescribes.
3. **Consistency** — matches the file's existing `unwrap_or_else` / defensive style around the
   shared mutex; no new patterns introduced.
4. **Maintainability** — minimal, localized change; behavior in the edge case is self-evident
   from the code (no pruning when nothing could be prunable).
5. **Completeness** — fully resolves B-M2; both the panic and its root cause (unchecked
   duration subtraction) are eliminated.
6. **Performance** — no measurable change; one extra `Option` branch per call.
7. **Security** — removes a crash-on-request-path bug reachable by any client hitting
   `/api/v1/auth/login` shortly after process boot (potential DoS vector prior to the fix).
8. **API Currency** — `Instant::checked_sub` is stable std, no external API involved.
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
