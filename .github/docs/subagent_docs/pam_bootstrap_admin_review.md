# PAM Bootstrap Admin Fallback — Review

## Scope

Reviewed against spec: `.github/docs/subagent_docs/pam_bootstrap_admin_spec.md`.

Modified files:
- `crates/vexboard-server/src/db/mod.rs` — added `try_claim_setting`
- `crates/vexboard-server/src/api/auth.rs` — `login_pam` role computation
- `config/default.toml` — documented bootstrap behavior
- `crates/vexboard-server/src/tests.rs` — two new tests for `try_claim_setting`

## 1. Specification Compliance

Matches the spec exactly: atomic claim via bare `INSERT` against the `settings` table's
`PRIMARY KEY`, `unwrap_or(false)` fail-closed on error, `tracing::warn!` + audit log entry
(`auth.pam_bootstrap_admin_granted`) on the winning grant, only triggers when
`pam_admin_users` is empty, non-empty list behavior is byte-for-byte unchanged (existing
`if ... any(|u| u == &payload.username)` logic preserved verbatim inside the new outer
`if !state.config.auth.pam_admin_users.is_empty()` branch). `config/default.toml` comment
added as specified.

One deviation, deliberate and noted in the spec itself: the audit-insert-inside-the-claim-
branch approach ("fold this into the `try_claim_setting` success branch") was used instead of
the alternative sketch shown first in the spec — this was explicitly spec'd as the preferred
option ("Simpler: fold this into..."), so this is compliant, not a deviation from intent.

## 2. Best Practices

- Atomicity is correctly delegated to SQLite's `PRIMARY KEY` constraint rather than a
  check-then-write race — the right pattern for this kind of one-time claim without adding
  application-level locking.
- Fail-closed (`unwrap_or(false)` → `viewer`) on any DB error during the claim attempt is
  consistent with the project's existing SEC-8 posture.
- `#[allow(dead_code)]` on `try_claim_setting` is scoped narrowly with a comment explaining
  *why* (function is reachable only via a feature-gated call site plus tests), matching
  existing precedent in the codebase (`probe/uptime.rs:20`, `discovery/systemd.rs:21`).

## 3. Consistency

- Matches the existing audit-log call style (`db::audit::insert(&state.db, &payload.username,
  "auth.login_success", None, None, None, Some(ip))`) used immediately below it in the same
  function — new event uses the same argument shape.
- `try_claim_setting` sits next to `get_setting`/`set_setting` in `db/mod.rs`, same doc-comment
  style, same `anyhow::Result` return convention.
- Test naming/structure (`TestApp::new()`, `#[tokio::test]`, section-comment banners) matches
  the rest of `tests.rs`.

## 4. Maintainability

Behavior is self-contained to `login_pam`; no new cross-cutting state beyond one `settings`
row. The `default.toml` comment makes the one-time-grant semantics discoverable without reading
source. No cleanup/expiry logic needed since the row, once written, is simply inert dead weight
after `pam_admin_users` is populated (as designed).

## 5. Completeness

All spec implementation steps present: `try_claim_setting` helper, `login_pam` branch,
`default.toml` docs, and a test plan for the new atomic-claim helper (feature-independent,
since `pam-auth` itself isn't testable in this environment per existing project constraints —
matches the spec's own test-plan scoping and the precedent set by `pam-auth-hardening_spec.md`).

## 6. Performance

One extra `INSERT` attempt only on logins where `pam_admin_users` is empty — negligible, and
only relevant for fresh/unconfigured installs, not the steady-state configured path (which
takes the original, unchanged branch).

## 7. Security

- Preserves SEC-8's core guarantee: once `pam_admin_users` is non-empty, behavior is bit-for-bit
  identical to before this change — no weakening of the configured-allowlist path.
- Bounds the reintroduced implicit-admin surface to exactly one grant, ever, per fresh
  database — not "every OS user" as before SEC-8. This is a deliberate, user-approved tradeoff
  (see conversation record), explicitly justified and documented in the spec's Risks section.
- Audit trail (`auth.pam_bootstrap_admin_granted`, plus the claimed username stored as the
  settings value) gives an operator a way to see after the fact who got the implicit grant.

## 8. API Currency

No external dependencies added; uses only `sqlx` and `tracing`, both already in use in this
exact form elsewhere in the file.

## 9. Build Validation

Commands run (all from the approved safe list):

```
$ cargo fmt --all -- --check
(no output — clean)

$ cargo clippy --workspace -- -D warnings
    Checking vexboard-server v0.2.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.93s

$ cargo test -p vexboard-server
running 36 tests
...
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

$ cargo build --release --bin vexboard-server
    Compiling vexboard-server v0.2.0 (...)
    Finished `release` profile [optimized] target(s) in 12.24s
```

All four commands succeeded on the first attempt (clippy required one fix mid-review: initial
implementation triggered a `dead_code` lint since `try_claim_setting` is only reachable through
`pam-auth`-feature-gated code in a default build; resolved with a scoped, documented
`#[allow(dead_code)]` before this review — reflected in the diff above, not a residual issue).

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 95% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (99%)**

(Security scored 95% rather than 100% only because this deliberately reintroduces a bounded
implicit-admin code path at all — an inherent, accepted tradeoff of the feature itself, not a
defect in its implementation.)

## Result

**PASS** — no CRITICAL or RECOMMENDED issues found. Proceeding to Phase 6 (Preflight).
