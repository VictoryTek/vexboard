# Phase 1 Spec: Consolidate Feature-Gated Auth Handler Duplication

**Feature:** auth_dedup  
**Date:** 2026-06-06  
**Audit Entry:** 2.3.2

---

## Current State

`crates/vexboard-server/src/api/auth.rs` defines three handlers (`login`, `me`,
`update_me`) each duplicated as a `#[cfg(all(unix, feature = "pam-auth"))]` and
a `#[cfg(not(all(unix, feature = "pam-auth")))]` pair — six top-level functions
total, ~230 lines of near-duplicate code.

**Duplication by handler:**

| Handler | PAM lines | Local lines | Shared logic |
|---------|-----------|-------------|--------------|
| `login` | 30 | 50 | Rate limit check, IP extraction, session insert, audit log |
| `me` | 10 | 15 | Session username read, 401 path |
| `update_me` | 5 (stub → 405) | 90 | None — genuinely different |

## Problem

Any cross-cutting change (new audit event, 2FA, new session field) must be
applied to both the PAM and local branch of `login` and `me`.

## Proposed Solution

### `login` — single handler, private cfg-gated helpers

Extract credential verification into private helpers
`login_pam()` / `login_local()`. The shared logic (IP extraction, rate limit
check) lives once in the public `login` handler, which delegates to one of
the helpers based on the active feature set.

Return type of helpers: `(StatusCode, Json<serde_json::Value>)` — identical
across both paths, avoiding the need for an enum or trait.

### `me` — single handler, inline cfg

The only divergence is role source (`"admin"` hardcoded vs. session read) and
`auth_mode` string. Use a single function body with a cfg-gated assignment of
`(role, auth_mode)` inside the match arm.

### `update_me` — keep feature-gated (intentional)

The PAM version takes zero args and returns 405. The local version takes
`State + Session + Json<UpdateMeRequest>`. Axum's extractor machinery requires
the real signature; making both use the full signature would force Axum to
parse the request body in PAM mode (caller sends no body → 422 before handler
runs). This is the one legitimate case for cfg-gated signatures.

## Implementation Steps

1. Replace the two `login` top-level functions with:
   - One `pub(crate) async fn login(...)` with shared preamble
   - `#[cfg(all(unix, feature = "pam-auth"))] async fn login_pam(...)`
   - `#[cfg(not(...))] async fn login_local(...)`

2. Replace the two `me` top-level functions with one `pub(crate) async fn me(session: Session)`.

3. Keep `update_me` feature-gated — add a comment explaining why.

4. Keep a single `#[utoipa::path]` annotation on each public handler (unified).

## Dependencies

None new.

## Build / Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `scripts/preflight.sh`

## Risks

Low. Logic is unchanged; only structure moves. Compiler enforces correctness.
The PAM feature is not enabled in this environment, so only the local paths are
compiled in CI — the PAM paths are verified by type-checking alone.
