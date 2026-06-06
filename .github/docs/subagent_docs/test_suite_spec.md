# Phase 1 Spec: Integration Test Suite Expansion

**Feature:** test_suite
**Audit Entry:** 2.4.2
**Date:** 2026-06-06

---

## Current State Analysis

The workspace has 2 unit tests, both in `discovery/systemd.rs` testing the `is_excluded` string
helper. Zero API handler tests exist. `cargo test -p vexboard-server` compiles and runs test
discovery, but SIGSEGVs at runtime in this environment due to a known zbus/D-Bus issue
(documented in preflight as `[WARN]`). The code compiles — tests just can't execute here.

---

## Problem Definition

With 2 tests for ~4,500 lines of code, `cargo test` is a compilation check with no regression
safety. Auth, middleware, services CRUD, and role enforcement are entirely untested.

---

## Design Decisions

### Test location

`src/tests.rs` referenced from `main.rs` via `#[cfg(test)] mod tests;`.  
This gives access to all `pub(crate)` items without converting the binary to a lib crate.

### Database

SQLite `:memory:` pool — `SqlitePool::connect(":memory:")` — isolated per test, no disk I/O,
no cleanup. Requires `db::run_migrations` to be `pub(crate)`.

### Session middleware

`tower_sessions::MemoryStore` — the built-in Arc-backed in-memory store included with
`tower-sessions 0.15`. Simpler than `SqliteSessionStore` for tests and sufficient for
validating session-cookie round-trips.

### ConnectInfo

`auth::login` extracts `ConnectInfo<SocketAddr>`. With `.oneshot()`, extensions are empty.
Inserting a fake `ConnectInfo` into `req.extensions_mut()` before dispatch satisfies the
extractor without changing production code.

### Multi-request sequences (login then act)

Router is `Clone`; `MemoryStore` is `Arc`-backed. Pattern:
1. `app.clone().oneshot(req1)` → login → extract `Set-Cookie` header
2. `app.clone().oneshot(req2.header("cookie", cookie))` → authenticated request

### Rate limiting

Test config sets `login_rate_limit_attempts: 0` (disabled) to avoid triggering 429 in tests.

### Password hashing

Use `bcrypt::hash(pw, 4)` in `seed_admin`/`seed_viewer` — cost 4 is the minimum, takes ~1 ms
vs ~100 ms at cost 12.

---

## Test Infrastructure (`src/tests.rs`)

```rust
struct TestApp { pool: SqlitePool, app: Router }

impl TestApp {
    async fn new() -> Self
    async fn seed_admin(&self, username, password)
    async fn seed_viewer(&self, username, password)
    async fn login(&self, username, password) -> (StatusCode, String /* cookie */)
    async fn get(&self, uri, cookie) -> (StatusCode, serde_json::Value)
    async fn post_json(&self, uri, body, cookie) -> (StatusCode, serde_json::Value)
    async fn delete(&self, uri, cookie) -> (StatusCode, serde_json::Value)
}
```

---

## Test Cases

### Auth

| Test | Expected |
|------|----------|
| `health_check` | 200 |
| `login_success` | 200, body has `user.username` |
| `login_wrong_password` | 401 |
| `login_unknown_user` | 401 |
| `me_unauthenticated` | 401 |
| `me_authenticated` | 200, body has correct username and role |
| `logout_invalidates_session` | login → logout → /me → 401 |

### Middleware enforcement

| Test | Expected |
|------|----------|
| `services_unauthenticated_returns_401` | GET /api/v1/services → 401 |
| `admin_route_as_viewer_returns_403` | POST /api/v1/services as viewer → 403 |

### Services CRUD

| Test | Expected |
|------|----------|
| `list_services_empty` | 200, `[]` |
| `create_service_as_admin` | 201, body has `id` |
| `create_service_as_viewer_returns_403` | 403 |
| `delete_service_as_admin` | 200 |

---

## Files to modify / create

1. `crates/vexboard-server/src/db/mod.rs` — make `run_migrations` `pub(crate)`
2. `crates/vexboard-server/src/main.rs` — add `#[cfg(test)] mod tests;`
3. `crates/vexboard-server/src/tests.rs` — new test file

---

## Dependencies

No new Cargo dependencies.
- `tower::ServiceExt` — already in `[dependencies]` (`tower = { version = "0.5", features = ["util"] }`)
- `tower_sessions::MemoryStore` — already in `[dependencies]` (`tower-sessions = "0.15"`)
- `axum::body::to_bytes` — Axum 0.8 built-in
- `serde_json` — already in `[dependencies]`

Context7 not required.

---

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `bash scripts/preflight.sh`

Note: tests compile but SIGSEGV at runtime in this environment (pre-existing D-Bus issue,
handled by preflight `[WARN]`). Compilation success verifies correctness.

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| SIGSEGV prevents test execution | Confirmed pre-existing | Tests compile and are valid; SIGSEGV is environment-specific, not code-related |
| `ConnectInfo` extractor fails | None | Fake extension inserted in every test request |
| `MemoryStore` session race in parallel tests | None | Each `TestApp::new()` creates its own isolated store |
