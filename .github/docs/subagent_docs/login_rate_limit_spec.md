# Login Rate Limiting — Specification
**Feature:** Per-IP rate limiting on the login endpoint (audit item 2.2.3)
**Date:** 2026-06-05
**Phase:** 1 — Research & Specification

---

## Current State Analysis

`crates/vexboard-server/src/api/auth.rs` — the `/api/v1/auth/login` POST handler performs
bcrypt verification with no request rate limiting. An attacker can submit unlimited login
attempts per second, enabling brute-force and credential stuffing attacks.

There is no existing rate-limiting middleware or abstraction in the project.

---

## Problem Definition

The login endpoint is the only credential-accepting surface in the application.
Without per-IP throttling, an attacker with network access can enumerate passwords
at full CPU speed. Even with bcrypt cost 12 (~250 ms/hash) a single-threaded
attacker gets ~4 attempts/sec; with parallel connections this rises significantly.

---

## Dependency Decision

`tower-governor` was evaluated but is not in the local cargo registry (never downloaded
for this project). `tower-governor` compatibility with Axum 0.8 / tower 0.5 cannot be
verified without network access or Context7, so it is excluded per the audit's
resource-constraint policy.

**Chosen approach:** zero-dependency custom sliding-window rate limiter implemented
directly in the server crate using only standard library + tokio primitives already
in the dependency graph.

---

## Proposed Solution Architecture

### 1. `crates/vexboard-server/src/rate_limit.rs` (new file)

```rust
pub struct LoginRateLimiter {
    state: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    max_attempts: u32,
    window: Duration,
}

impl LoginRateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self { ... }
    /// Returns true if the request is allowed, false if rate-limited.
    pub fn check(&self, ip: IpAddr) -> bool { ... }
}
```

Sliding window: on each call, evict entries older than `window`, then check count.
Uses `std::sync::Mutex` (not tokio — the critical section is microseconds; no await
inside the lock).

### 2. `AppState` — add `login_limiter: Arc<LoginRateLimiter>`

The limiter lives in shared state so all request handler clones share the same counter.

### 3. `AuthConfig` — add rate limit config fields

```toml
# Max login attempts per IP per window before returning 429.
login_rate_limit_attempts = 10
# Sliding window duration in seconds.
login_rate_limit_window_secs = 60
```

With `#[serde(default)]` so existing configs without these keys work unchanged.

### 4. `api/auth.rs` — check limiter in login handler

Extract client IP from `ConnectInfo<SocketAddr>` (direct connections) with fallback
to `X-Forwarded-For` header (reverse proxy deployments). If `check()` returns false,
return `429 Too Many Requests` before any DB or bcrypt work.

### 5. `main.rs` — switch to `into_make_service_with_connect_info::<SocketAddr>()`

Required for `ConnectInfo` extraction to work in handlers.

### 6. `config/default.toml` — document the new knobs under `[auth]`

---

## Implementation Steps

1. Create `crates/vexboard-server/src/rate_limit.rs`
2. Edit `crates/vexboard-server/src/config.rs` — add two fields to `AuthConfig`
3. Edit `config/default.toml` — add `login_rate_limit_attempts` and `login_rate_limit_window_secs` under `[auth]`
4. Edit `crates/vexboard-server/src/main.rs`:
   - Add `mod rate_limit;`
   - Construct `LoginRateLimiter` from config after loading config
   - Add `login_limiter` to `AppState`
   - Switch `into_make_service()` → `into_make_service_with_connect_info::<SocketAddr>()`
5. Edit `crates/vexboard-server/src/api/auth.rs` — add IP extraction + limiter check to both login variants

---

## Dependencies

No new dependencies. Uses only:
- `std::collections::{HashMap, VecDeque}`
- `std::net::IpAddr`
- `std::sync::Mutex`
- `std::time::{Duration, Instant}`
- `axum::extract::ConnectInfo` (already in `axum`)
- `axum::http::HeaderMap` (already in `axum`)

---

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| IP spoofing via X-Forwarded-For | Prefer ConnectInfo for direct connections; only fall back to X-Forwarded-For header. Document that deployers behind a trusted reverse proxy should ensure the proxy strips the header from external requests. |
| Memory growth (many attacker IPs) | VecDeque entries are evicted after the window; per-IP cost is at most `max_attempts * size_of::<Instant>()` = trivial |
| Lock contention | Mutex held for microseconds (no I/O inside); acceptable for a single-binary self-hosted dashboard |

---

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build --release --bin vexboard-server`
