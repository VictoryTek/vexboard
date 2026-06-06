# OpenAPI / Swagger UI — Specification
**Phase:** 1 — Research & Specification
**Date:** 2026-06-05
**Feature:** Feature Recommendation #5 from project_audit_2026-06-04

---

## 1. Current State Analysis

The VexBoard REST API has no machine-readable specification. All 20+ endpoints exist
exclusively in source code; third-party integrations and testing require reading Rust
handler files directly. No `/swagger-ui`, `/api-docs`, or spec generation exists today.

### Existing API surface (all under `crates/vexboard-server/src/api/`)

| Module | Handler(s) | Method + Path |
|---|---|---|
| `health` | `health_check` | `GET /health` |
| `setup` | `status`, `create_admin` | `GET /api/v1/setup/status`, `POST /api/v1/setup` |
| `auth` | `login`, `logout`, `me`, `update_me` | `POST /api/v1/auth/login`, `POST /api/v1/auth/logout`, `GET PATCH /api/v1/auth/me` |
| `services` | `list_services`, `create_service`, `update_service`, `delete_service`, `claim_service` | `GET POST /api/v1/services`, `PUT DELETE /api/v1/services/{id}`, `POST /api/v1/services/{id}/claim` |
| `groups` | `list_groups`, `create_group`, `update_group`, `delete_group` | `GET POST /api/v1/groups`, `PUT DELETE /api/v1/groups/{id}` |
| `quick_links` | `list_quick_links`, `create_quick_link`, `update_quick_link`, `delete_quick_link` | `GET POST /api/v1/quick-links`, `PUT DELETE /api/v1/quick-links/{id}` |
| `metrics` | `metrics_snapshot`, `metrics_stream` | `GET /api/v1/metrics/snapshot`, `GET /api/v1/metrics/stream` (SSE) |
| `audit` | `list_audit` | `GET /api/v1/audit` |
| `discovery` | `list_discovered`, `trigger_refresh` | `GET /api/v1/discovery`, `POST /api/v1/discovery/refresh` |

Current dependency tree: no `utoipa`, `utoipa-swagger-ui`, or any OpenAPI crate present
in either `Cargo.toml`.

---

## 2. Problem Definition

- REST API has no machine-readable specification → external integration requires reading source code
- No interactive testing UI for development or QA
- API contracts are implicit and undocumented

---

## 3. Research Findings

### 3.1 Library Versions (verified via docs.rs and lib.rs, 2026-06-05)

| Crate | Version | Notes |
|---|---|---|
| `utoipa` | **5.5.0** | `axum_extras` feature for axum-native `IntoParams`; `chrono` feature for `NaiveDateTime` schema |
| `utoipa-swagger-ui` | **9.0.2** | `axum` feature bundles pre-configured `SwaggerUi` router; works with axum 0.8 |
| `utoipa-axum` | 0.2.0 | `OpenApiRouter` pattern — NOT selected (see §3.2) |

### 3.2 Integration Approach Decision

Two approaches exist:

**Option A — `utoipa-axum` OpenApiRouter pattern**
- Replace all `router() -> Router<AppState>` functions with `router() -> OpenApiRouter<AppState>`
- Use `routes!(handler_fn)` macro per handler registration
- Call `.split_for_parts()` at the top level to extract `(Router, OpenApi)`
- Pros: automatic path collection, no central list to maintain
- Cons: invasive change to every `router()` function across 7+ modules; adds risk surface; incompatible with Axum `IntoResponse` dispatch of private fns

**Option B — Central `#[derive(OpenApi)]` + `SwaggerUi::merge` (SELECTED)**
- Keep all existing `router()` functions unchanged
- Add `#[utoipa::path(...)]` attribute above each handler fn (annotations only, no signature changes)
- Add `#[derive(utoipa::ToSchema)]` to model structs in `db/models.rs` and `discovery/mod.rs`
- Create new `crates/vexboard-server/src/api/openapi.rs` with a central `ApiDoc` struct annotated `#[derive(OpenApi)]` that lists all paths and schemas
- Merge `SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())` into the public router in `api/mod.rs`
- Pros: zero risk to existing routing; surgical change; follows official utoipa examples
- Cons: central `ApiDoc` schema/paths list must be maintained manually

**Option B is selected.** Zero functional risk; easiest to review and verify.

### 3.3 Feature Flags Required

```toml
# utoipa — chrono: NaiveDateTime schema support; axum_extras: IntoParams without parameter_in
utoipa = { version = "5", features = ["axum_extras", "chrono"] }
# utoipa-swagger-ui — axum: bundles SwaggerUi type + pre-built WASM assets
utoipa-swagger-ui = { version = "9", features = ["axum"] }
```

### 3.4 SSE Endpoint Handling

OpenAPI 3.x does not natively represent SSE streams. `GET /api/v1/metrics/stream` will be
documented with `content_type = "text/event-stream"` and a description noting SSE semantics.
The `GET /api/v1/metrics/snapshot` REST endpoint is fully documented with a JSON schema.

### 3.5 PAM-conditional Handlers

Several handlers (`login`, `me`, `update_me`, `status`, `create_admin`) have
`#[cfg(all(unix, feature = "pam-auth"))]` and `#[cfg(not(...))]` variants. Only the
`#[cfg(not(all(unix, feature = "pam-auth")))]` variant (the default compile path) receives
`#[utoipa::path]` annotation. This is consistent with how the build normally runs.

### 3.6 Session/Cookie Security Scheme

All protected routes require an active session cookie. The OpenAPI spec will declare a
`cookieAuth` security scheme and mark protected endpoints with `security(("cookieAuth" = []))`.

---

## 4. Proposed Solution Architecture

```
crates/vexboard-server/src/
├── api/
│   ├── openapi.rs          ← NEW: ApiDoc derive, central schema/path registry
│   ├── mod.rs              ← MODIFIED: pub mod openapi; SwaggerUi merge in router()
│   ├── auth.rs             ← MODIFIED: #[utoipa::path] on login, logout, me, update_me
│   ├── services.rs         ← MODIFIED: #[utoipa::path] on all 5 handlers
│   ├── groups.rs           ← MODIFIED: #[utoipa::path] on all 4 handlers
│   ├── quick_links.rs      ← MODIFIED: #[utoipa::path] on all 4 handlers
│   ├── audit.rs            ← MODIFIED: #[utoipa::path] on list_audit
│   ├── health.rs           ← MODIFIED: #[utoipa::path] on health_check
│   ├── setup.rs            ← MODIFIED: #[utoipa::path] on status, create_admin
│   └── metrics.rs          ← MODIFIED: #[utoipa::path] on snapshot + stream
├── db/
│   └── models.rs           ← MODIFIED: ToSchema derives on all DTO structs
└── discovery/
    └── mod.rs              ← MODIFIED: ToSchema on DiscoveredUnit
Cargo.toml (workspace)      ← MODIFIED: add utoipa, utoipa-swagger-ui
crates/vexboard-server/Cargo.toml ← MODIFIED: reference workspace deps
```

---

## 5. Implementation Steps

### Step 1 — Workspace & server Cargo.toml

Add to workspace `Cargo.toml` `[workspace.dependencies]`:
```toml
utoipa = { version = "5", features = ["axum_extras", "chrono"] }
utoipa-swagger-ui = { version = "9", features = ["axum"] }
```

Add to `crates/vexboard-server/Cargo.toml` `[dependencies]`:
```toml
utoipa = { workspace = true }
utoipa-swagger-ui = { workspace = true }
```

### Step 2 — `db/models.rs`: add `ToSchema` derives

Add `utoipa::ToSchema` to the following structs (all already `Serialize`/`Deserialize`):
- `Group`, `Service`, `User` (DB row types)
- `CreateService`, `UpdateService`
- `CreateGroup`, `UpdateGroup`
- `LoginRequest`, `UserInfo`
- `ServiceWithStatus`
- `QuickLink`, `CreateQuickLink`, `UpdateQuickLink`
- `AuditEvent`

`User` struct contains `password_hash` — add `#[schema(value_type = String, write_only = true)]` on
the `password_hash` field so it is declared write-only in the schema (best practice).

`NaiveDateTime` fields will resolve automatically via the `chrono` feature.

### Step 3 — `discovery/mod.rs`: add `ToSchema` to `DiscoveredUnit`

`DiscoveredUnit` is `Serialize` already; add `utoipa::ToSchema` derive.

### Step 4 — `api/openapi.rs` (new file)

```rust
use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::Modify;

struct SecurityAddon;
impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "cookieAuth",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("session_id"))),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "VexBoard API",
        version = "0.1.0",
        description = "Self-hosted server dashboard REST API",
        license(name = "MIT"),
        contact(name = "VexBoard", email = "victorytech@proton.me"),
    ),
    paths(
        // health
        crate::api::health::health_check,
        // setup
        crate::api::setup::status,
        crate::api::setup::create_admin,
        // auth
        crate::api::auth::login,
        crate::api::auth::logout,
        crate::api::auth::me,
        crate::api::auth::update_me,
        // services
        crate::api::services::list_services,
        crate::api::services::create_service,
        crate::api::services::update_service,
        crate::api::services::delete_service,
        crate::api::services::claim_service,
        // groups
        crate::api::groups::list_groups,
        crate::api::groups::create_group,
        crate::api::groups::update_group,
        crate::api::groups::delete_group,
        // quick_links
        crate::api::quick_links::list_quick_links,
        crate::api::quick_links::create_quick_link,
        crate::api::quick_links::update_quick_link,
        crate::api::quick_links::delete_quick_link,
        // audit
        crate::api::audit::list_audit,
        // metrics
        crate::api::metrics::metrics_snapshot,
        crate::api::metrics::metrics_stream,
        // discovery
        crate::discovery::list_discovered,
        crate::discovery::trigger_refresh,
    ),
    components(
        schemas(
            crate::db::models::Group,
            crate::db::models::Service,
            crate::db::models::CreateService,
            crate::db::models::UpdateService,
            crate::db::models::CreateGroup,
            crate::db::models::UpdateGroup,
            crate::db::models::LoginRequest,
            crate::db::models::UserInfo,
            crate::db::models::ServiceWithStatus,
            crate::db::models::QuickLink,
            crate::db::models::CreateQuickLink,
            crate::db::models::UpdateQuickLink,
            crate::db::models::AuditEvent,
            crate::discovery::DiscoveredUnit,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Health check"),
        (name = "setup", description = "Initial admin setup"),
        (name = "auth", description = "Authentication"),
        (name = "services", description = "Service management"),
        (name = "groups", description = "Group management"),
        (name = "quick-links", description = "Quick link management"),
        (name = "audit", description = "Audit log (protected)"),
        (name = "metrics", description = "System metrics"),
        (name = "discovery", description = "Auto-discovery"),
    )
)]
pub struct ApiDoc;
```

**Note:** The `openapi.rs` file must be in `api/` with `pub mod openapi;` in `api/mod.rs`, and all
referenced handler functions and types must be `pub` (currently many are private `async fn`).
The handlers need to be made `pub(crate)` (or `pub`) so `openapi.rs` can reference them by path in
the `paths(...)` list. This is the **only structural change** to existing handlers.

### Step 5 — Promote handlers from private to `pub(crate)`

The `#[utoipa::path]` macro on a function does not require the function to be public.
However, the `paths(crate::api::services::list_services, ...)` references inside the
`#[openapi(...)]` derive DO require the functions to be accessible from the crate root.

Change all private handler functions (`async fn`) to `pub(crate) async fn` in:
- `api/auth.rs`
- `api/services.rs`
- `api/groups.rs`
- `api/quick_links.rs`
- `api/audit.rs`
- `api/health.rs`
- `api/setup.rs`
- `api/metrics.rs`
- `discovery/mod.rs` (for `list_discovered`, `trigger_refresh`)

Alternatively, if `pub(crate)` conflicts with Axum handler registration in the router, use
`pub` instead — Axum accepts both.

### Step 6 — Add `#[utoipa::path]` annotations to every handler

Each handler gets an annotation specifying:
- HTTP method and path (matching the Axum route)
- Tag (matching the `tags(...)` list in `ApiDoc`)
- Request body (if POST/PUT/PATCH)
- Path/query parameters (if any)
- Responses with status codes and response body schema

Examples:

```rust
// Health check (no auth)
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Service is healthy", body = inline(serde_json::Value)),
        (status = 503, description = "Database unreachable", body = inline(serde_json::Value)),
    )
)]
pub(crate) async fn health_check(...) { ... }

// List services (cookieAuth protected)
#[utoipa::path(
    get,
    path = "/api/v1/services",
    tag = "services",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "List of services with probe status", body = Vec<ServiceWithStatus>),
        (status = 401, description = "Not authenticated"),
    )
)]
pub(crate) async fn list_services(...) { ... }

// Create service
#[utoipa::path(
    post,
    path = "/api/v1/services",
    tag = "services",
    security(("cookieAuth" = [])),
    request_body = CreateService,
    responses(
        (status = 201, description = "Service created", body = inline(serde_json::Value)),
        (status = 500, description = "Database error"),
    )
)]
pub(crate) async fn create_service(...) { ... }

// Update service (path param)
#[utoipa::path(
    put,
    path = "/api/v1/services/{id}",
    tag = "services",
    security(("cookieAuth" = [])),
    params(("id" = i64, Path, description = "Service ID")),
    request_body = UpdateService,
    responses(
        (status = 200, description = "Updated"),
        (status = 404, description = "Not found"),
    )
)]
pub(crate) async fn update_service(...) { ... }

// Audit list (query params)
#[utoipa::path(
    get,
    path = "/api/v1/audit",
    tag = "audit",
    security(("cookieAuth" = [])),
    params(
        ("limit" = i64, Query, description = "Max rows returned (1–500, default 50)"),
        ("offset" = i64, Query, description = "Pagination offset (default 0)"),
    ),
    responses(
        (status = 200, description = "Paginated audit log entries", body = inline(serde_json::Value)),
    )
)]
pub(crate) async fn list_audit(...) { ... }

// SSE stream
#[utoipa::path(
    get,
    path = "/api/v1/metrics/stream",
    tag = "metrics",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Server-sent events stream of SystemSnapshot objects",
         content_type = "text/event-stream"),
    )
)]
pub(crate) async fn metrics_stream(...) { ... }
```

### Step 7 — `api/mod.rs`: register openapi module and wire SwaggerUi

```rust
pub mod openapi;

// inside router():
use utoipa_swagger_ui::SwaggerUi;
use openapi::ApiDoc;
use utoipa::OpenApi;

// Add to the public Router::new() chain (before returning):
.merge(SwaggerUi::new("/swagger-ui")
    .url("/api-docs/openapi.json", ApiDoc::openapi()))
```

The SwaggerUi routes are public — they serve static assets and the spec JSON, which does
not expose sensitive data beyond what the spec itself declares.

---

## 6. Dependencies

| Crate | Version | Feature flags | Purpose |
|---|---|---|---|
| `utoipa` | 5.5.0 | `axum_extras`, `chrono` | Path/schema macro, OpenAPI derive |
| `utoipa-swagger-ui` | 9.0.2 | `axum` | Swagger UI assets + axum Router integration |

No new runtime dependencies beyond Swagger UI static assets (bundled via the crate).

---

## 7. Configuration Changes

None. No TOML config or environment variable changes needed.
The Swagger UI is always enabled; disabling it would require a feature flag
(not in scope for this feature recommendation).

---

## 8. Build and Test Commands (Phase 3)

All commands are approved safe commands per CLAUDE.md. None are FORBIDDEN.

| Command | Purpose | Resource cost |
|---|---|---|
| `cargo fmt --all -- --check` | Formatting check | Negligible — no compilation |
| `cargo clippy --workspace -- -D warnings` | Lint + type check | Low — compiles server crate for native target only |
| `cargo build --release --bin vexboard-server` | Full release binary | Medium — one binary, no WASM |
| `cargo test --workspace` | Unit + integration tests | Low (SIGSEGV in binary runner is pre-existing, not a regression) |

`cargo audit --ignore RUSTSEC-2023-0071` may be run if `cargo-audit` is installed to check new dependencies for known CVEs.

---

## 9. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Handler visibility change (`pub(crate)`) breaks routing | Axum accepts any async fn in `.route()` regardless of visibility qualifier; verified by inspection |
| `NaiveDateTime` schema missing | Addressed by `features = ["chrono"]` on utoipa |
| `#[serde(flatten)]` in `ServiceWithStatus` not supported | utoipa 5.x supports `flatten` — verified in changelog; may need `#[schema(flatten)]` alias if clippy warns |
| `User.password_hash` exposed in schema | Mitigated by `#[schema(value_type = String, write_only = true)]` on `password_hash` field |
| PAM-mode builds fail due to mismatched handler signatures | `#[utoipa::path]` annotations placed only on the `#[cfg(not(all(unix, feature = "pam-auth")))]` variants — the default build path; PAM builds are excluded by feature flag |
| SSE endpoint undocumentable in strict OpenAPI 3.0 | Document with `content_type = "text/event-stream"` and description; spec remains valid |
| `User` struct in schema registry leaks DB-level field | `User` is intentionally excluded from the `components(schemas(...))` list — only `UserInfo` (the safe DTO) is registered |

---

## 10. File Inventory

Files to be created:
- `crates/vexboard-server/src/api/openapi.rs`

Files to be modified:
- `Cargo.toml` (workspace)
- `crates/vexboard-server/Cargo.toml`
- `crates/vexboard-server/src/db/models.rs`
- `crates/vexboard-server/src/api/mod.rs`
- `crates/vexboard-server/src/api/health.rs`
- `crates/vexboard-server/src/api/setup.rs`
- `crates/vexboard-server/src/api/auth.rs`
- `crates/vexboard-server/src/api/services.rs`
- `crates/vexboard-server/src/api/groups.rs`
- `crates/vexboard-server/src/api/quick_links.rs`
- `crates/vexboard-server/src/api/audit.rs`
- `crates/vexboard-server/src/api/metrics.rs`
- `crates/vexboard-server/src/discovery/mod.rs`
