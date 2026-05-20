# Dependency Upgrade Review — VexBoard Workspace

**Date:** 2026-05-20  
**Reviewer:** Automated Review Agent  
**Scope:** Full workspace build, lint, test, format, code correctness after dependency version upgrades

---

## Build Exit Codes

| Step | Command | Exit Code | Result |
|------|---------|-----------|--------|
| 1. Backend Build | `cargo build --release --bin vexboard-server` | **101** | ❌ FAILED |
| 2. Workspace Lint | `cargo clippy --workspace -- -D warnings` | **101** | ❌ FAILED |
| 3. Workspace Tests | `cargo test --workspace` | **101** | ❌ FAILED (compile error) |
| 4. Format Check | `cargo fmt --all -- --check` | **1** | ❌ FAILED |
| 5. Frontend Build | `trunk build --release` | DEFERRED | Trunk not installed locally — deferred to Docker build |

---

## Build Output — Step 1: Backend Build

The build compiled all 400+ dependency crates successfully up to `vexboard-server`. Compilation failed with **4 errors and 4 warnings** in the project sources:

```
error: couldn't read `crates\vexboard-server\src\db\db/migrations/001_init.sql`
  --> crates\vexboard-server\src\db\mod.rs:34:20
  |
34 |     let init_sql = include_str!("db/migrations/001_init.sql");
  help: there is a file with the same name in a different directory
34 +     let init_sql = include_str!("migrations/001_init.sql");

error[E0433]: cannot find `ManagerProxy` in `fdo`
  --> crates\vexboard-server\src\discovery\systemd.rs:40:28
  |
40 |     let proxy = zbus::fdo::ManagerProxy::builder(&connection)
  |                            ^^^^^^^^^^^^ could not find `ManagerProxy` in `fdo`

error[E0308]: mismatched types
   --> crates\vexboard-server\src\api\services.rs:261:5
  (claim_service returns impl IntoResponse but early-returns (StatusCode, Json<Value>)
   causing two incompatible concrete types for the opaque return type)
  = note: expected tuple `(reqwest::StatusCode, axum::Json<JsonValue>)`
          found opaque type `impl IntoResponse`

error[E0277]: the trait bound `fn(State<AppState>, ...) -> ... {update_service}: Handler<_, _>` is not satisfied
   --> crates\vexboard-server\src\api\services.rs:16:29
   (downstream cascade from the mismatched-types error above)

warning: unused imports: `delete` and `post`
 --> crates\vexboard-server\src\api\groups.rs:5:15

warning: unused import: `crate::metrics::system::SystemSnapshot`
  --> crates\vexboard-server\src\api\metrics.rs:17:5

warning: unused import: `delete`
 --> crates\vexboard-server\src\api\services.rs:5:15

warning: unused variable: `state`
  --> crates\vexboard-server\src\api\metrics.rs:45:33
```

**Backend build: FAILED (exit 101)**

---

## Build Output — Step 2: Workspace Lint (Clippy)

Clippy surfaced all the same backend errors plus **additional frontend errors**:

### Frontend errors (`vexboard-frontend`):

```
error[E0277]: &std::string::String: leptos::prelude::IntoRender is not satisfied
  --> crates\vexboard-frontend\src\components\service_card.rs:35:57
  {&service.display_name} — &String doesn't implement IntoRender in Leptos 0.8

error[E0277]: &std::string::String: leptos::prelude::IntoRender is not satisfied
  --> crates\vexboard-frontend\src\components\service_card.rs:44:25
  {&latency_text} — same issue

error[E0277]: ServiceResponse: serde::Serialize is not satisfied
  --> crates\vexboard-frontend\src\pages\dashboard.rs:19:20
  Resource::new requires T: Serialize; ServiceResponse doesn't derive Serialize

error[E0308]: `if` and `else` have incompatible types
  --> crates\vexboard-frontend\src\pages\dashboard.rs:41:25
  Empty-services branch returns View<HtmlElement<Div,...,(InertElement,InertElement)>>
  Service-list branch returns View<...Vec<View<...>>>
  These opaque view types cannot be unified

error[E0277]: &str: leptos_router::PossibleRouteMatch is not satisfied
  --> crates\vexboard-frontend\src\main.rs:22:36  (path="/")
  --> crates\vexboard-frontend\src\main.rs:23:36  (path="/settings")
  --> crates\vexboard-frontend\src\main.rs:24:36  (path="/login")
  In Leptos Router 0.8, plain &str is not a valid route path; requires typed segments

error: unused variable: `set_metrics`
  --> crates\vexboard-frontend\src\components\metric_bar.rs:16:19
  set_metrics is only used inside #[cfg(target_arch = "wasm32")] block;
  outside wasm32 it is dead code (-D warnings makes this an error)
```

**Clippy: FAILED (exit 101) — 8 server errors + 16 frontend errors**

---

## Build Output — Step 3: Tests

Tests failed to compile for the same reasons as the build. No tests could execute.

**Tests: FAILED (exit 101)**

---

## Build Output — Step 4: Format Check

`cargo fmt --all -- --check` reported formatting diffs in the following files (exit 1):

| File | Nature of Diff |
|------|---------------|
| `crates/vexboard-frontend/src/main.rs` | Import sort order: `Route, Router, Routes` |
| `crates/vexboard-server/src/api/groups.rs` | Multi-line fn signature flattened; match arm style |
| `crates/vexboard-server/src/api/health.rs` | Method chain reformatting |
| `crates/vexboard-server/src/api/mod.rs` | Import sort order |
| `crates/vexboard-server/src/api/services.rs` | `tags_json` binding, delete fn signature, delete match arm, `exists` query chain, `claim_service` query chain |
| `crates/vexboard-server/src/discovery/mod.rs` | `ACCEPTED` tuple multi-line formatting |
| `crates/vexboard-server/src/discovery/systemd.rs` | `discovery_loop` fn signature; claimed query chain |
| `crates/vexboard-server/src/probe/uptime.rs` | INSERT query chain |

**Format check: FAILED (exit 1)**

---

## Version Correctness Analysis

Dependency versions in `Cargo.toml` were evaluated against resolved versions in the build output and `cargo search` results.

| Crate | Spec Version | Resolved | Latest | Status |
|-------|-------------|----------|--------|--------|
| axum | `^0.8` | 0.8.9 | 0.8.9 | ✅ |
| zbus | `^5` | 5.15.0 | 5.15.0 | ✅ |
| reqwest | `^0.13` | 0.13.1 | 0.13.3 | ⚠️ lock behind by 2 patches |
| thiserror | `^2` | 2.0.18 | 2.0.18 | ✅ |
| tower | `^0.5` | 0.5.3 | 0.5.3 | ✅ |
| tower-http | `^0.6` | 0.6.11 | 0.6.11 | ✅ |
| tower-sessions | `^0.15` | 0.15.0 | 0.15.0 | ✅ |
| bcrypt | `^0.19` | 0.19.1 | 0.19.1 | ✅ |
| config | `^0.15` | 0.15.23 | 0.15.23 | ✅ |
| leptos | `^0.8` | 0.8.19 | 0.8.19 | ✅ |
| leptos_router | `^0.8` | 0.8.13 | 0.8.13 | ✅ |
| gloo-net | `^0.7` | 0.7.0 | 0.7.0 | ✅ |
| gloo-timers | `^0.4` | 0.4.0 | 0.4.0 | ✅ |

**Notes:**
- The `reqwest` lock resolves to `0.13.1` instead of `0.13.3`. Running `cargo update reqwest` would pull in the latest patch.
- Previously broken features (`rustls-tls-native-roots` for reqwest; `csr` for `leptos_router`) are **corrected** in the current `Cargo.toml` — reqwest now uses `rustls`+`rustls-native-certs`, and `leptos_router` has no invalid `csr` feature.
- Version correctness for `Cargo.toml` declarations is **good**; the lockfile has one minor stale patch.

---

## API Correctness Review

### Axum Route Strings
- `services.rs:15-16`: `.route("/", ...).route("/{id}", ...)` — ✅ uses `{id}` syntax (Axum 0.8 style)
- `groups.rs:14-15`: `.route("/", ...).route("/{id}", ...)` — ✅ correct

### zbus Named Fields
- `systemd.rs:52-57`: Accesses `unit.name`, `unit.description`, `unit.load_state`, `unit.active_state`, `unit.sub_state` as named fields — ✅ correct pattern for struct-based proxy
- **CRITICAL**: The proxy type `zbus::fdo::ManagerProxy` does not exist in zbus v5. `zbus::fdo` only contains D-Bus FDO standard interface proxies (not systemd). The systemd `Manager` interface proxy must come from `zbus_systemd::systemd1::ManagerProxy` (external crate) or a local `#[dbus_proxy]`-annotated trait.

### Uptime Probe HEAD→GET Fallback
- `probe/uptime.rs:40-64`: HEAD request first; on any `Err(_)`, falls back to GET — ✅ clean implementation

### Leptos 0.8 Patterns
| Pattern | File | Status |
|---------|------|--------|
| `use leptos::prelude::*` | All frontend files | ✅ |
| `signal(` | login.rs, sidebar.rs, metric_bar.rs, modal_edit.rs | ✅ |
| `Effect::new(` | metric_bar.rs | ✅ |
| `Resource::new(` | dashboard.rs | ✅ |
| `mount_to_body(\|\| view! { <App/> })` | main.rs | ✅ |
| `<Routes fallback=...>` | main.rs | ✅ |
| `<Route path=...>` with typed segments | main.rs | ❌ uses `&str` literal instead of `leptos_router::path!()` |

---

## Detailed Issue List

### CRITICAL Issues — Server

**[C-1]** `crates/vexboard-server/src/db/mod.rs:34`  
Wrong `include_str!` path: `"db/migrations/001_init.sql"` resolves to `src/db/db/migrations/...` which does not exist.  
**Fix:** `include_str!("migrations/001_init.sql")`

**[C-2]** `crates/vexboard-server/src/discovery/systemd.rs:40`  
`zbus::fdo::ManagerProxy` does not exist in zbus v5. The `fdo` module in zbus 5 contains only standard D-Bus FDO interface proxies, not the systemd Manager interface.  
**Fix Option A:** Add `zbus_systemd = "0.15"` to `Cargo.toml` and replace with `zbus_systemd::systemd1::ManagerProxy`.  
**Fix Option B:** Define a custom proxy locally:
```rust
#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait Manager {
    fn list_units(&self) -> zbus::Result<Vec<UnitInfo>>;
}
#[derive(Debug, zvariant::Type, serde::Deserialize)]
struct UnitInfo {
    pub name: String, pub description: String, pub load_state: String,
    pub active_state: String, pub sub_state: String, pub followed: String,
    pub object_path: zvariant::OwnedObjectPath, pub queued_job_id: u32,
    pub job_type: String, pub job_object_path: zvariant::OwnedObjectPath,
}
```

**[C-3]** `crates/vexboard-server/src/api/services.rs:261`  
`claim_service` has two return paths with different concrete types behind `impl IntoResponse`:
- Early return: `(StatusCode::CONFLICT, Json<Value>)` — concrete tuple
- Tail expression: `create_service(...).await` — opaque `impl IntoResponse`

Rust cannot unify these. The router registration of `update_service` (line 16) also fails as a downstream cascade.  
**Fix:** Use `axum::response::Response` as the explicit return type and call `.into_response()` on all return paths:
```rust
use axum::response::Response;
async fn claim_service(...) -> Response {
    // ...
    return (StatusCode::CONFLICT, Json(json!({...}))).into_response();
    // ...
    create_service(State(state), Json(payload)).await.into_response()
}
```

**[C-4]** `crates/vexboard-server/src/api/groups.rs:5`  
Unused imports `delete` and `post` — treated as errors by `-D warnings`.  
**Fix:** Remove `delete` and `post` from the import list (only `get` and `put` are used).

**[C-5]** `crates/vexboard-server/src/api/metrics.rs:17`  
Unused import `crate::metrics::system::SystemSnapshot` — treated as error.  
**Fix:** Remove the import (the function `metrics_snapshot` uses the qualified path instead).

**[C-6]** `crates/vexboard-server/src/api/services.rs:5`  
Unused import `delete` — treated as error.  
**Fix:** Remove `delete` from the import list.

**[C-7]** `crates/vexboard-server/src/api/metrics.rs:45`  
Unused variable `state` in `metrics_snapshot` handler — treated as error.  
**Fix:** Rename to `_state` or remove the `State` extractor if it's genuinely unused.

---

### CRITICAL Issues — Frontend

**[C-8]** `crates/vexboard-frontend/src/components/service_card.rs:35`  
`{&service.display_name}` — in Leptos 0.8 (tachys renderer), `&String` does not implement `IntoRender`. Static non-reactive values must be owned.  
**Fix:** `{service.display_name.clone()}`

**[C-9]** `crates/vexboard-frontend/src/components/service_card.rs:44`  
`{&latency_text}` — same `&String` `IntoRender` issue.  
**Fix:** `{latency_text.clone()}` or just `{latency_text}` (move semantics, since `latency_text` is not used after).

**[C-10]** `crates/vexboard-frontend/src/pages/dashboard.rs:7`  
`ServiceResponse` derives only `Deserialize`, but `Resource::new` in Leptos 0.8 requires `T: Serialize` (for `JsonSerdeCodec`).  
**Fix:** Add `Serialize` to the derive macro:
```rust
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct ServiceResponse { ... }
```

**[C-11]** `crates/vexboard-frontend/src/pages/dashboard.rs:33-56`  
The two branches of the `if svcs.is_empty()` expression inside `.map()` return incompatible opaque view types (`View<HtmlElement<Div,...,(InertElement,InertElement)>>` vs `View<...Vec<...>>`). Leptos 0.8 cannot unify these.  
**Fix:** Use `either_of` helpers or `EitherOf2`:
```rust
use leptos::either::EitherOf2;
if svcs.is_empty() {
    EitherOf2::A(view! { <div>...</div> })
} else {
    EitherOf2::B(view! { <div>...</div> })
}
```

**[C-12]** `crates/vexboard-frontend/src/main.rs:22-24`  
`Route path="/"` (and `/settings`, `/login`) — in Leptos Router 0.8, `&str` does not implement `PossibleRouteMatch`. Plain string literals are not valid route paths.  
**Fix:** Use the `path!` macro from Leptos Router 0.8:
```rust
use leptos_router::path;
<Route path=path!("/") view=pages::dashboard::DashboardPage />
<Route path=path!("/settings") view=pages::settings::SettingsPage />
<Route path=path!("/login") view=pages::login::LoginPage />
```

**[C-13]** `crates/vexboard-frontend/src/components/metric_bar.rs:16`  
`set_metrics` is declared via `signal()` but only referenced inside `#[cfg(target_arch = "wasm32")]`. Outside WASM, it is unused, and `-D warnings` makes this a hard error.  
**Fix:** Prefix with underscore inside the non-wasm path, or move the full `signal` declaration inside the `#[cfg(target_arch = "wasm32")]` block:
```rust
#[cfg(target_arch = "wasm32")]
let (_metrics, set_metrics) = signal(SystemMetrics::default());
#[cfg(not(target_arch = "wasm32"))]
let (metrics, _) = signal(SystemMetrics::default());
```
Or restructure: keep `signal` outside but name the setter `_set_metrics`.

---

### RECOMMENDED Issues

**[R-1]** Multiple files fail `cargo fmt --all -- --check` (exit 1). While not blocking compilation, CI pipelines typically enforce formatting. Run `cargo fmt --all` to fix:
- `crates/vexboard-frontend/src/main.rs` — import sort
- `crates/vexboard-server/src/api/groups.rs` — fn signature, match style
- `crates/vexboard-server/src/api/health.rs` — method chain
- `crates/vexboard-server/src/api/mod.rs` — import sort
- `crates/vexboard-server/src/api/services.rs` — multiple style diffs
- `crates/vexboard-server/src/discovery/mod.rs` — tuple formatting
- `crates/vexboard-server/src/discovery/systemd.rs` — fn signature, query chain
- `crates/vexboard-server/src/probe/uptime.rs` — query chain

**[R-2]** `crates/vexboard-server/src/api/services.rs:117-125`  
`sets` and `binds` vectors are declared and partially populated but are completely ignored — the function proceeds to a full-record fetch-then-overwrite approach. This is dead code. Either remove `sets`/`binds` entirely or implement the dynamic partial-update query.

**[R-3]** `reqwest` lock resolves to `0.13.1` but `0.13.3` is available. Run `cargo update reqwest` to get the latest patch.

---

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Build Success | 0% | F |
| Lint (-D warnings) | 0% | F |
| Tests | 0% | F |
| Version Correctness (Cargo.toml) | 88% | B+ |
| Code Quality (API patterns, logic) | 45% | D |
| Formatting | 0% | F |
| **Overall** | **22%** | **F** |

---

## Final Verdict

**NEEDS_REFINEMENT**

The dependency version upgrades in `Cargo.toml` are correct — all 13 target crates are at their latest SemVer-compatible versions, and previously broken feature flags (`rustls-tls-native-roots`, `leptos_router csr`) are resolved. However, the source code contains **13 critical compile errors** preventing any build from succeeding.

All 13 critical issues must be resolved before the build can pass. The highest-priority fixes are:

1. **[C-2]** `zbus::fdo::ManagerProxy` — does not exist in zbus v5; requires a custom `#[dbus_proxy]` or `zbus_systemd` crate
2. **[C-1]** `db/mod.rs` include_str path — one-line fix
3. **[C-3]** `claim_service` return type — use `axum::response::Response`
4. **[C-12]** Leptos Router 0.8 route paths — use `path!()` macro
5. **[C-4 through C-7]** Unused imports/variable — remove them
6. **[C-8, C-9]** `&String` not `IntoRender` in Leptos 0.8 view macro — use owned values
7. **[C-10]** `ServiceResponse` missing `Serialize` — add to derive
8. **[C-11]** Incompatible if/else view types — use `EitherOf2`
9. **[C-13]** `set_metrics` unused outside wasm32 — prefix with `_` or restructure
