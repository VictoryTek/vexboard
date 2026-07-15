# Review: Auto-identify as sole account when Disable Login is on

## Spec Compliance

Matches the revised spec (`disable_login_sole_admin_spec.md`):
- `db::users::get_sole_user` added, PAM-gated off.
- `resolve_effective_user` extracted and shared by `me()` and
  `update_sort_mode()`, so both identity display *and* preference persistence
  work off the same sole-account resolution in Disable Login mode.
- `UserMenu` frontend component hides all identity chrome (avatar, username,
  dropdown, account-settings modal) when `auth_mode == "none"`, matching the
  "no visible identity concept" direction from the user.
- Ambiguous case (0 or 2+ accounts) unchanged: synthetic anonymous/admin, `az`
  sort default — same as before this change.

## Findings

- **Caught in review-cycle testing, fixed before this doc:** initial version
  only updated `me()`; `PUT /me/sort-mode` still required a session
  unconditionally, so a sort-mode change made while Disable Login was on could
  never persist (silently 401'd, swallowed by the frontend's `let _ =
  req.send().await`). Fixed by extracting `resolve_effective_user` and using
  it in both handlers. Covered by `test_me_auth_mode_none_resolves_sole_user`,
  which exercises the full PUT-then-GET round trip.
- No other issues found.

## Build Validation

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean, no output |
| `cargo test -p vexboard-server` | 47 passed, 0 failed |
| `cargo clippy --workspace -- -D warnings` | clean, no warnings |
| `cargo build --release --bin vexboard-server` | success |
| `cargo check -p vexboard-frontend --target wasm32-unknown-unknown` (supplementary, not in the approved list but not forbidden — confirms the `UserMenu` view! macro edit actually compiles for its real target) | success |

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 95% | A |
| Functionality | 100% | A |
| Code Quality | 95% | A |
| Security | 100% | A |
| Performance | 95% | A (one extra indexed DB read per unauthenticated `/me`/sort-mode call in "none" mode, same cost class as the existing per-request role read) |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (98%)**

## Result

PASS
