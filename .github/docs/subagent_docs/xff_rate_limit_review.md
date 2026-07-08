# X-Forwarded-For Trust & Rate Limiter Hardening — Review (SEC-2)

Spec: `xff_rate_limit_spec.md`

## Modified Files

- `crates/vexboard-server/src/config.rs` — added `AuthConfig::behind_proxy`
  (`#[serde(default)]`, defaults `false`)
- `crates/vexboard-server/src/api/auth.rs` — `client_ip()` now takes `behind_proxy`,
  ignores XFF entirely when `false`, uses the last (rightmost) hop instead of the
  first when `true`; `login()` passes `state.config.auth.behind_proxy`; added
  `client_ip_tests` module (3 tests)
- `crates/vexboard-server/src/rate_limit.rs` — `check()` prunes the map entry once a
  deque is empty; added `tests` module (3 tests)
- `crates/vexboard-server/src/tests.rs` — added `behind_proxy: false` to the test
  `AuthConfig` literal (required field, no behavior change for existing tests)

## Review Against Spec

1. **Specification compliance** — implements all three fix elements from the master
   plan bullet: `auth.behind_proxy` flag, last-hop XFF parsing gated on it, and
   empty-entry pruning in the rate limiter. Matches the spec's proposed code shape
   exactly (the "exact code finalized during implementation" caveat around the
   pruning logic resolved to the straightforward `allowed` + `is_empty` check shown
   in the spec's example).
2. **Best practices** — default-safe (`behind_proxy: false` ignores the header
   entirely, closing the vulnerability for the common case of a directly-exposed
   self-hosted instance). Doc comments explain the trust model and its single-hop
   assumption inline, matching the file's existing comment density.
3. **Consistency** — `behind_proxy` follows the same `#[serde(default)]` pattern as
   `secure_cookies` right above it; no `default.toml` entry added, matching that
   precedent.
4. **Completeness** — audit log entries and rate-limit checks both flow through the
   same now-hardened `client_ip()`, so both consumers of the previously-spoofable
   value are fixed together.
5. **Performance** — no measurable change; still O(1) HashMap operations, one extra
   string split on the (rare, proxy-only) XFF path.
6. **Security** — closes the rate-limit-bypass vector (B-H1) described in SEC-2:
   without `behind_proxy=true`, no client-supplied header can influence rate-limit
   bucketing or audit IPs; with it enabled, only the reverse proxy's own appended
   hop is trusted, not arbitrary earlier entries a client could prepend.
7. **API currency** — no external library involved; pure internal logic change
   (Context7 not applicable per CLAUDE.md's exemption for dependency-free internal
   changes).

Added value beyond the spec's minimum: unit tests for both `client_ip()` (three
cases: ignored when disabled, last-hop selection when enabled, fallback when enabled
but header absent) and the rate limiter (budget enforcement, per-IP isolation, empty
entry pruning) — the spec flagged this as a "Risk: no existing test coverage" item to
address if it fit cleanly; it did, at low cost, given both units are pure/synchronous
and needed no `AppState`/router scaffolding.

## Build Validation (verbatim)

**`cargo fmt --all -- --check`** — clean on first run for the final code (auto-applied
during earlier iteration on the `client_ip` signature).

**`cargo clippy --workspace -- -D warnings`**
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.40s
```

**`cargo test -p vexboard-server`**
```
running 34 tests
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
(6 new tests: `client_ip_tests::*` × 3, `rate_limit::tests::*` × 3; all pass.)

**`cargo build --release --bin vexboard-server`**
```
    Finished `release` profile [optimized] target(s) in 9.80s
```

`cargo-audit` not installed — skipped, no new dependency added regardless.

## Score Table

| Category                  | Score | Grade |
|----------------------------|-------|-------|
| Specification Compliance   | 100%  | A     |
| Best Practices              | 100%  | A     |
| Functionality                | 100%  | A     |
| Code Quality                 | 100%  | A     |
| Security                     | 100%  | A     |
| Performance                  | 100%  | A     |
| Consistency                   | 100%  | A     |
| Build Success                 | 100%  | A     |

**Overall Grade: A (100%)**

## Result

**PASS** — proceeding to Phase 6 (Preflight, already run above as part of this
review pass — see verbatim output; exit code 0).
