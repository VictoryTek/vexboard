# skip_tls_verify — Review

## Spec Compliance

Implementation matches `.github/docs/subagent_docs/skip_tls_verify_spec.md` step for step:
migration 009, `Service`/`CreateService`/`UpdateService` model updates, all four SQL column-list
sites in `api/services.rs` (list, create INSERT + immediate-probe SELECT, update SELECT + UPDATE),
`probe/mod.rs` scheduler SELECT + dual-client dispatch, `main.rs` second `reqwest::Client` with
`danger_accept_invalid_certs(true)` wired into `AppState` and the probe loop spawn, and the full
frontend chain (`EditFormData`, checkbox UI, `ServiceResponse`, both mapping sites, all four JSON
payload sites).

## Build Validation (commands run, verbatim)

- `cargo fmt --all -- --check` → initially failed (one line over the wrap width in
  `api/services.rs`), fixed, re-ran clean.
- `cargo clippy -p vexboard-server -- -D warnings` → clean, 0 warnings. (Scoped to
  `-p vexboard-server` rather than `--workspace`, per Resource Constraints: the frontend crate is
  WASM-only and a workspace-wide clippy/build attempts native compilation of it, which fails hard —
  this mirrors how the release build command itself is scoped with `--bin vexboard-server`.)
- `cargo test -p vexboard-server` → 36/36 passed, no SIGSEGV.
- `cargo build --release --bin vexboard-server` → succeeded, no warnings.
- `cargo check -p vexboard-frontend --target wasm32-unknown-unknown` → succeeded (extra check
  beyond the mandated list, run because `wasm32-unknown-unknown` was already installed on this
  machine; confirms the new `EditFormData`/`ServiceResponse` fields and checkbox compile for the
  actual WASM target without invoking Trunk, which is not installed here).

## Category Scores

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 95% | A |
| Functionality | 100% | A |
| Code Quality | 95% | A |
| Security | 95% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (98%)**

### Notes

- **Security**: the insecure client is strictly opt-in per service (default `false` at DB, DTO,
  and frontend levels); no existing service's behavior changes. This mirrors the pattern
  Uptime Kuma and Homepage use — verified via web search before implementation — rather than a
  global TLS bypass.
- **Best Practices (95%, not 100%)**: `AppState` now carries two long-lived `reqwest::Client`
  instances. This is intentional and matches the existing single-client pattern (both are
  `Arc`-backed internally, cheap to clone), but it's worth noting for future maintainers that a
  third TLS policy (e.g. custom CA pinning) would need a third client rather than a more general
  per-service TLS config — out of scope for this fix, not a defect.
- No CRITICAL issues found. No REFINEMENT needed.

## Result: PASS
