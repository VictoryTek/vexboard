# Phase 1 Spec: Extract Auth Middleware into Dedicated Module

**Feature:** middleware_extract  
**Date:** 2026-06-06  
**Audit Entry:** 2.3.1

---

## Current State

`require_auth` and `require_admin` are private `async fn` in
`crates/vexboard-server/src/api/mod.rs` (lines 24–56). They are called via
`middleware::from_fn(require_auth)` and `middleware::from_fn(require_admin)` in
the same file's `router()` function. They cannot be reused by any code outside
`api/mod.rs` without moving them.

## Problem

Auth policy lives in the router-aggregation file. Any future middleware
(logging, rate-limiting at router level, RBAC extensions) would either pile into
`api/mod.rs` or end up scattered. The audit notes this as a maintainability
concern: policy changes require knowing to look in the router file.

## Proposed Solution

Create a `middleware` top-level module:

```
crates/vexboard-server/src/
  middleware/
    mod.rs      ← pub use auth::{require_auth, require_admin};
    auth.rs     ← the two async fn, now pub
  api/
    mod.rs      ← use crate::middleware::auth::{require_auth, require_admin};
  main.rs       ← add: mod middleware;
```

No logic changes — purely mechanical extraction. Both functions' signatures and
bodies are unchanged.

## Implementation Steps

1. Create `crates/vexboard-server/src/middleware/auth.rs` — move both functions, add `pub`
2. Create `crates/vexboard-server/src/middleware/mod.rs` — `pub mod auth`
3. Edit `crates/vexboard-server/src/main.rs` — add `mod middleware;`
4. Edit `crates/vexboard-server/src/api/mod.rs`:
   - Remove the two private `async fn` definitions
   - Add `use crate::middleware::auth::{require_auth, require_admin};`
   - Remove now-unused `tower_sessions::Session` import (it moves to `middleware/auth.rs`)

## Dependencies

None new.

## Build / Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `scripts/preflight.sh`

## Risks

Minimal. Purely mechanical; no observable behavior change.
