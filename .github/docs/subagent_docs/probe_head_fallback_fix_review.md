# Phase 3 Review — probe_head_fallback_fix

**Date:** 2026-07-03

## Problem Statement

`probe_service` (the HTTP prober for URL-based/"remote" services) only fell back from
HEAD to GET when the HEAD *request* errored at the transport level. A HEAD request that
received a non-2xx/3xx HTTP response (405 Method Not Allowed being especially common)
was recorded as `"down"` immediately, with GET never attempted — misreporting reachable
services as down. Errors were also discarded with no logging, making the failure mode
undiagnosable from server logs.

---

## Modified Files

| File | Change |
|------|--------|
| `crates/vexboard-server/src/probe/uptime.rs` | `probe_service`: GET fallback now triggers on any non-success HEAD outcome (error or non-2xx/3xx status), and both the fallback trigger and final GET failure are logged via `tracing` |

---

## Review Criteria

### 1. Specification Compliance — 100% / A

Spec called for restructuring the HEAD/GET match in `probe_service` exactly as
implemented, with logging added and `danger_accept_invalid_certs` explicitly left
unchanged. Diff matches the spec's proposed code precisely. No scope creep — no other
function in the file touched, dispatch sites (`probe/mod.rs`, `api/services.rs`)
untouched, no schema/frontend changes.

### 2. Best Practices — 100% / A

- Matches Uptime Kuma's approach of not letting a single-method-not-allowed response
  masquerade as "service down."
- Logging style matches the existing precedent in `probe_systemd_unit`
  (`tracing::warn!` with structured fields on failure).
- No new dependencies; `tracing` already a direct dependency and already used in this
  file.

### 3. Functionality — 100% / A

- HEAD returns 2xx/3xx → "up", unchanged.
- HEAD returns non-2xx/3xx (e.g. 405) → now falls back to GET instead of prematurely
  recording "down" (the actual bug fix).
- HEAD request errors (DNS/timeout/connection/TLS) → falls back to GET, unchanged
  behavior, now with a `tracing::debug!` breadcrumb.
- GET ultimately fails or returns non-success → "down", now with a `tracing::warn!`
  including the concrete error or status, closing the previous diagnosability gap.

### 4. Code Quality — 100% / A

- `cargo fmt --all -- --check` → PASS
- `cargo clippy --workspace -- -D warnings` → PASS, 0 warnings (full workspace type-checks, including the WASM frontend crate, without linking — consistent with prior review's documented behavior)
- Single self-contained match restructuring; no new functions or abstractions introduced for a one-call-site fix (Simplicity First)

### 5. Security — 100% / A

No new attack surface. TLS certificate validation behavior (`danger_accept_invalid_certs(false)`) is explicitly unchanged, per the user's "not sure" answer on whether their affected services use self-signed certs — deferred to a follow-up informed by the new error logging rather than silently weakened here.

### 6. Performance — 100% / A

Only newly-affected case (non-success HEAD status) now issues a second request (GET) before recording "down" — one extra round trip at most, negligible at the default 30s probe interval. Transport-error fallback path is unchanged (already did this).

### 7. Consistency — 100% / A

Logging style, field naming (`url = %url`, `error = %e`), and warn/debug level choices match existing conventions in the same file (`probe_systemd_unit`).

### 8. Build Validation

```
cargo fmt --all -- --check                    → PASS
cargo clippy --workspace -- -D warnings       → PASS (0 warnings)
cargo test -p vexboard-server                 → PASS (28 passed; 0 failed; no SIGSEGV)
cargo build --release --bin vexboard-server   → PASS (Finished `release` profile in 9.78s)
```

The linker environment issue noted as BLOCKED in the prior `remote_service_status_fix_review.md` (2026-07-02, broken Nix-store `ld-wrapper.sh` path) is no longer present — the release build linked and finished successfully on this run.

---

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

## Result: PASS
