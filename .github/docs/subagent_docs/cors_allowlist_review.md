# CORS Origin Allowlist — Review
**Feature:** Configurable CORS origin allowlist (audit item 2.2.2)
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

---

## Findings

### Implementation
- `ServerConfig.allowed_origins: Vec<String>` with `#[serde(default = "default_allowed_origins")]` — fully backward-compatible; existing deployments without this key default to `["*"]`
- Default of `["*"]` maps to `CorsLayer::allow_origin(Any)` — identical to previous hardcoded behavior
- Explicit origins are parsed to `HeaderValue` with a `tracing::warn!` for any malformed entry (no silent failure)
- Config comment in `default.toml` documents both the wildcard form and the production restriction form

### Security
- Closes the audit finding: production deployments can now restrict CORS to a known frontend origin
- Malformed origins are skipped with a warning rather than panicking or silently allowing them
- `["*"]` default is intentional for the self-hosted local-network use case described in the audit

### Consistency
- Follows the same pattern as `secure_cookies` — a flag in `AuthConfig` with a safe default and a doc comment explaining when to change it

---

## Result: **PASS**
