# Optional Auth Mode — Review

## Spec Compliance

Implementation matches `optional_auth_mode_spec.md` step-for-step:

1. `AuthConfig.mode: String` added with `default_auth_mode() -> "session"` — `crates/vexboard-server/src/config.rs`.
2. `config/default.toml` gained `[auth].mode = "session"` with the exact bidirectional comment specified (enable *and* revert instructions).
3. `api::router()` now takes `auth_mode: &str`; both `route_layer` wraps are conditional on `auth_mode == "none"` — `crates/vexboard-server/src/api/mod.rs`.
4. `main.rs` passes `&config.auth.mode`, and emits `tracing::warn!` once at startup when mode is `"none"`.
5. Invalid `mode` values fail fast via `anyhow::bail!` in `AppConfig::load()`, matching the existing error style (`load()` already returns `anyhow::Result`).

One deviation from the spec's literal pseudocode: the spec sketched `router(auth_mode: &str)` re-declaring `viewer`/`admin` before conditionally wrapping. The actual implementation builds `viewer_protected`/`admin_protected` once, then shadows the binding with the conditional — functionally identical, slightly less duplication. Not a compliance issue.

Additional change not explicitly itemized in the spec but required for correctness: `tests.rs` had a hand-built `AppConfig` struct literal (not going through `Deserialize`, so `#[serde(default)]` doesn't apply) and a direct `api::router()` call with no arguments. Both were updated (`mode: "session".to_string()` field added; call site changed to `api::router("session")`). This is a necessary consequence of the signature change in step 3, not scope creep.

## Best Practices / Consistency

- New field follows the exact `#[serde(default = "...")]` + free function pattern already used for every other optional `AuthConfig` field (`login_rate_limit_attempts`, `login_rate_limit_window_secs`).
- Validation added at the single existing config-loading choke point (`AppConfig::load()`), not scattered — consistent with "fail fast, once."
- `router()` branches once at construction, not per-request — no new runtime overhead on the hot path when auth is enabled (matches spec's stated rationale over per-request middleware branching).
- Config comment mirrors the file's existing comment density/style (see `secure_cookies`, `server_services_only` for precedent).

## Completeness

All 5 implementation steps done. Non-goals honored: no per-route granularity added, no IP allowlisting, `pam-auth` untouched.

## Security

- Default behavior unchanged (`mode` defaults to `"session"` both via serde default and in `default.toml`) — existing deployments see zero behavior change without explicit opt-in.
- Invalid/typo'd `mode` values reject at startup rather than silently falling back to either state — prevents "silently open" and "silently still gated when the operator thought they disabled it" failure modes alike.
- Startup log line makes the disabled state observable in logs, addressing the spec's stated risk of an operator forgetting they enabled it.
- SPA static-asset fallback correctly left untouched — it was already unauthenticated pre-change, no regression introduced there.

## Performance

No measurable impact — the conditional is evaluated once at router construction (startup), not per-request.

## Build Validation

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass, no output |
| `cargo clippy --workspace -- -D warnings` | Pass, 0 warnings |
| `cargo test -p vexboard-server` | Pass, 28/28 (including `test_services_unauthenticated_returns_401`, `test_admin_route_as_viewer_returns_403` — confirm default `"session"` mode still gates correctly) |
| `cargo build --release --bin vexboard-server` | Pass, clean release build |
| `cargo audit --ignore RUSTSEC-2023-0071` | Skipped — cargo-audit not installed locally (optional per spec) |

No FORBIDDEN COMMANDS were run.

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

## Result

**PASS** — no CRITICAL or RECOMMENDED issues found. Proceeding directly to Phase 6 (Preflight); Phase 4/5 refinement not required.
