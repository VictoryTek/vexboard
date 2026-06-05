# CORS Origin Allowlist — Specification
**Feature:** Configurable CORS origin allowlist (audit item 2.2.2)
**Date:** 2026-06-05
**Phase:** 1 — Research & Specification

---

## Current State Analysis

`crates/vexboard-server/src/main.rs:121–126`:
```rust
let app = app.layer(
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any),
);
```

CORS is hardcoded to `Any` — it allows every origin unconditionally. In production behind a reverse proxy with a known frontend origin, this is a medium-high security liability (enables cross-origin requests from any site).

`crates/vexboard-server/src/config.rs` — `ServerConfig` has `host`, `port`, `assets_path`. No `allowed_origins` field.

`config/default.toml` — no `allowed_origins` key under `[server]`.

---

## Problem Definition

- Any web page can make credentialed cross-origin requests to the API.
- No configuration knob exists to restrict origins even when the deployment origin is known.
- The fix must be backward-compatible: existing deployments with no `allowed_origins` configured must continue to work (default = allow all).

---

## Proposed Solution Architecture

### 1. `config.rs` — add field to `ServerConfig`

```rust
#[serde(default = "default_allowed_origins")]
pub allowed_origins: Vec<String>,
```

```rust
fn default_allowed_origins() -> Vec<String> {
    vec!["*".to_string()]
}
```

`"*"` as the sole entry means "allow any origin" — maps to `CorsLayer::allow_origin(Any)`.
Any other value is treated as an explicit origin URL and mapped to `HeaderValue`.

### 2. `config/default.toml` — document the knob

Under `[server]`:
```toml
# CORS allowed origins. Use ["*"] to allow any origin (default, suitable for
# local-network deployments). In production, set this to your frontend URL:
#   allowed_origins = ["https://dashboard.example.com"]
allowed_origins = ["*"]
```

### 3. `main.rs` — wire config to CorsLayer

```rust
use axum::http::HeaderValue;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

let cors_layer = if config.server.allowed_origins.iter().any(|o| o == "*") {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
} else {
    let origins: Vec<HeaderValue> = config.server.allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
};
let app = app.layer(cors_layer);
```

---

## Implementation Steps

1. Edit `crates/vexboard-server/src/config.rs` — add `allowed_origins` to `ServerConfig` + default fn
2. Edit `config/default.toml` — add `allowed_origins = ["*"]` with comment under `[server]`
3. Edit `crates/vexboard-server/src/main.rs` — replace hardcoded `CorsLayer` with config-driven version; update imports

---

## Dependencies

No new dependencies. `tower-http` (already `0.6`) provides `CorsLayer` and `Any`. `axum::http::HeaderValue` is re-exported from the `http` crate already in the dependency graph via `axum`.

---

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Malformed origin strings silently dropped | `filter_map` logs nothing; use `tracing::warn!` if origin fails to parse |
| Default `["*"]` maintains exact existing behaviour | No regression for existing deployments |
| `HeaderValue` parsing rejects non-origin strings | Document in config comment that values must be full origin URLs (scheme + host) |

---

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build --release --bin vexboard-server`
