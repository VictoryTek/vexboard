# Login Rate Limiting — Review
**Feature:** Per-IP rate limiting on the login endpoint (audit item 2.2.3)
**Date:** 2026-06-05
**Phase:** 3 — Review & Quality Assurance

---

## Score Table

| Category | Score | Grade |
|---|---|---|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (100%)**

---

## Build Validation

- `cargo fmt --all -- --check` — ✅ PASS
- `cargo clippy --workspace -- -D warnings` — ✅ PASS (0 warnings)
- `cargo test --workspace` — ✅ PASS (2/2 tests pass)
- `cargo build --release --bin vexboard-server` — ✅ PASS
- `scripts/preflight.ps1` — ✅ PASS (all checks passed)

---

## Findings

### Implementation
- `rate_limit.rs`: Sliding-window `LoginRateLimiter` using `std::sync::Mutex<HashMap<IpAddr, VecDeque<Instant>>>`. Lock held for microseconds only; no I/O inside critical section.
- `VecDeque::front().is_some_and(...)` cleanly evicts expired entries before counting — correct sliding window semantics.
- `login_rate_limit_attempts = 0` disables the check entirely — useful for trusted internal deployments.
- IP extracted from `ConnectInfo<SocketAddr>` (direct connections) with `X-Forwarded-For` fallback (reverse proxy). Falls back gracefully.
- Both PAM and non-PAM login variants updated symmetrically.
- `into_make_service_with_connect_info::<SocketAddr>()` required for `ConnectInfo` extraction — correctly applied.

### Security
- Rate limit check runs before any DB query or bcrypt work — attacker cannot force expensive operations during a flood.
- `429 Too Many Requests` response reveals only that the limit was exceeded, not any credential information.
- `login_rate_limit_attempts` and `login_rate_limit_window_secs` are configurable in `config/default.toml` or via env vars.

### Zero new dependencies
- All primitives (`HashMap`, `VecDeque`, `Instant`, `Duration`, `Mutex`) are from `std`.
- `ConnectInfo` and `HeaderMap` are from `axum` and `axum::http` already in the graph.

---

## Result: **PASS**
