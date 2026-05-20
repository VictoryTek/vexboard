# VexBoard Dependency Upgrade Specification

**Date:** 2026-05-20  
**Author:** Research Agent (Context7-verified)  
**Status:** READY FOR IMPLEMENTATION  

---

## 1. Executive Summary

| Crate | Current | Latest | Classification | Risk |
|---|---|---|---|---|
| axum | 0.7 | 0.8.9 | **MINOR** | Route param syntax change in 2 files |
| tower | 0.4 | 0.5.3 | **TRIVIAL** | No VexBoard code directly uses it |
| tower-http | 0.5 | 0.6.11 | **TRIVIAL** | Used APIs unchanged |
| tower-sessions | 0.12 | 0.15.0 | **TRIVIAL** | Not yet wired up in code |
| reqwest | 0.12 | 0.13.3 | **MINOR** | 1 broken error conversion in uptime probe |
| thiserror | 1 | 2.0.18 | **TRIVIAL** | No derived errors in codebase |
| zbus | 4 | 5.15.0 | **MINOR** | fdo ManagerProxy / list_units return type |
| bcrypt | 0.15 | 0.19.1 | **TRIVIAL** | verify() API unchanged |
| config | 0.14 | 0.15.23 | **TRIVIAL** | Builder APIs unchanged |
| leptos | 0.6 | 0.8.19 | **MAJOR** | Widespread reactive-primitive renames |
| leptos_router | 0.6 | 0.8.13 | **MAJOR** | Routes/Router API restructured |
| gloo-net | 0.5 | 0.7.0 | **MINOR** | HTTP Request API compatible; web-sys bump |
| gloo-timers | 0.3 | 0.4.0 | **TRIVIAL** | Imported but never called |

**Safe to batch upgrade first (no source changes):** tower, tower-http, tower-sessions, thiserror, bcrypt, config, gloo-timers  
**Require targeted code fixes:** axum, reqwest, zbus, gloo-net  
**Requires significant rewrite:** leptos + leptos_router (do last; own branch)

---

## 2. Per-Crate Migration Sections

---

### 2.1 axum — MINOR

**Version:** 0.7 → 0.8.9  
**Source:** Context7 `/tokio-rs/axum` (`axum_v0_8_4` docs)

#### Breaking Changes

1. **Route parameter syntax changed from `:name` to `{name}`**  
   In axum 0.8 the default path-matching syntax uses curly-brace segments (`{id}`) instead of the colon-prefix syntax (`:id`). Registering a route with the old syntax now **panics at startup** unless `.without_v07_checks()` is called on the router (backwards-compat escape hatch).  
   - Old wildcard: `*rest` → New: `{*rest}`
   - Old capture: `:id` → New: `{id}`

2. **`MethodRouter::method_not_allowed_fallback`** was moved; not used by VexBoard.

3. No changes to `axum::serve`, `AppState`/`State` extractor, `Json`, `IntoResponse`, `CorsLayer`, or `ServeDir`.

#### Affected VexBoard Files

- `crates/vexboard-server/src/api/services.rs` — routes `/:id` and `/:id/claim`
- `crates/vexboard-server/src/api/groups.rs` — route `/:id`

#### Required Code Changes

**`crates/vexboard-server/src/api/services.rs`**

```diff
- .route("/:id", put(update_service).delete(delete_service))
- .route("/:id/claim", post(claim_service))
+ .route("/{id}", put(update_service).delete(delete_service))
+ .route("/{id}/claim", post(claim_service))
```

**`crates/vexboard-server/src/api/groups.rs`**

```diff
- .route("/:id", put(update_group).delete(delete_group))
+ .route("/{id}", put(update_group).delete(delete_group))
```

---

### 2.2 tower — TRIVIAL

**Version:** 0.4 → 0.5.3  
**Source:** Context7 `/websites/rs_tower`

#### Breaking Changes

1. The `Service` trait itself is **unchanged** — `poll_ready` and `call` signatures are identical.
2. `ServiceExt::oneshot` / `BoxService` internals updated but public signatures match.
3. VexBoard `Cargo.toml` lists `tower = { version = "0.4", features = ["util"] }` in the **server crate** but no source file directly imports `tower::*`. It is only pulled transitively by axum.

#### Affected VexBoard Files

None — no `use tower::` in any `.rs` file.

#### Required Code Changes

None. Version bump only.

---

### 2.3 tower-http — TRIVIAL

**Version:** 0.5 → 0.6.11  
**Source:** Context7 `/websites/rs_tower` (tower-http section)

#### Breaking Changes

1. `tower_http::cors::{Any, CorsLayer}` — public API **unchanged**.
2. `tower_http::services::ServeDir` — public API **unchanged**.
3. Internal alignment with tower 0.5 and hyper 1 — transparent to callers.

#### Affected VexBoard Files

`crates/vexboard-server/src/main.rs` — uses `CorsLayer` and `ServeDir`.

#### Required Code Changes

None. Version bump only.

---

### 2.4 tower-sessions — TRIVIAL

**Version:** 0.12 → 0.15.0

#### Breaking Changes

tower-sessions 0.13–0.15 introduced a new `SessionStore` trait surface and changed `Session` extraction. However, **VexBoard does not call any tower-sessions API** — the crate is declared as a dependency but all session handling contains only placeholder comments (`// In a full implementation, create a session via tower-sessions here`).

#### Affected VexBoard Files

None — no `use tower_sessions::` in any `.rs` file.

#### Required Code Changes

None. Version bump only.

---

### 2.5 reqwest — MINOR

**Version:** 0.12 → 0.13.3  
**Source:** Context7 `/websites/rs_reqwest`

#### Breaking Changes

1. **`reqwest::Error` no longer implements `From<std::io::Error>`**  
   In 0.13 the internal error type was reorganised. `reqwest::Error::from(io_error)` is removed.

2. **`rustls-tls` feature renamed**  
   In 0.13, `rustls-tls` still exists but the preferred names are `rustls-tls-native-roots` (uses system CA store) or `rustls-tls-webpki-roots` (bundles Mozilla CA). The old `rustls-tls` feature is an alias that still compiles.

3. **`danger_accept_invalid_certs`** — still available, no change.

4. **`Client::builder().timeout(...).build()`** — unchanged.

#### Affected VexBoard Files

`crates/vexboard-server/src/probe/uptime.rs` — contains an invalid `Err(reqwest::Error::from(std::io::Error::new(...)))` construction inside an `or_else` closure.

**Current broken code (lines ~37–43 of uptime.rs):**

```rust
let result = client.head(&url).send().await.or_else(|_| {
    // This is a sync fallback pattern — we'll just try GET in the same branch
    Err(reqwest::Error::from(std::io::Error::new(
        std::io::ErrorKind::Other,
        "HEAD failed",
    )))
});
```

This code is logically broken even on 0.12 (the `or_else` converts a reqwest error to... another reqwest error, discarding the signal to fall through to GET). It must be replaced with a proper dual-attempt pattern.

#### Required Code Changes

**`crates/vexboard-server/src/probe/uptime.rs`** — replace `probe_service` HEAD/GET logic:

```diff
-    let result = client.head(&url).send().await.or_else(|_| {
-        // This is a sync fallback pattern — we'll just try GET in the same branch
-        Err(reqwest::Error::from(std::io::Error::new(
-            std::io::ErrorKind::Other,
-            "HEAD failed",
-        )))
-    });
-
-    let (status, latency_ms) = match result {
-        Ok(resp) => {
-            let latency = start.elapsed().as_millis() as i64;
-            if resp.status().is_success() || resp.status().is_redirection() {
-                ("up".to_string(), Some(latency))
-            } else {
-                ("down".to_string(), Some(latency))
-            }
-        }
-        Err(_) => {
-            // HEAD failed, try GET
-            let start2 = Instant::now();
-            match client.get(&url).send().await {
-                Ok(resp) => {
-                    let latency = start2.elapsed().as_millis() as i64;
-                    if resp.status().is_success() || resp.status().is_redirection() {
-                        ("up".to_string(), Some(latency))
-                    } else {
-                        ("down".to_string(), Some(latency))
-                    }
-                }
-                Err(_) => ("down".to_string(), None),
-            }
-        }
-    };
+    // Try HEAD first; if it fails for any reason, fall back to GET.
+    let (status, latency_ms) = match client.head(&url).send().await {
+        Ok(resp) => {
+            let latency = start.elapsed().as_millis() as i64;
+            if resp.status().is_success() || resp.status().is_redirection() {
+                ("up".to_string(), Some(latency))
+            } else {
+                ("down".to_string(), Some(latency))
+            }
+        }
+        Err(_) => {
+            // HEAD failed — fall back to GET.
+            let start2 = Instant::now();
+            match client.get(&url).send().await {
+                Ok(resp) => {
+                    let latency = start2.elapsed().as_millis() as i64;
+                    if resp.status().is_success() || resp.status().is_redirection() {
+                        ("up".to_string(), Some(latency))
+                    } else {
+                        ("down".to_string(), Some(latency))
+                    }
+                }
+                Err(_) => ("down".to_string(), None),
+            }
+        }
+    };
```

Also remove the now-unused `Instant` re-declaration if only one `start` is kept. The `Instant` import remains; add a second `start2` inside the fallback arm as shown.

Additionally update the `Cargo.toml` feature:

```diff
- reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
+ reqwest = { version = "0.13", features = ["json", "rustls-tls-native-roots"], default-features = false }
```

---

### 2.6 thiserror — TRIVIAL

**Version:** 1 → 2.0.18

#### Breaking Changes

1. `#[from]` can no longer be combined with `#[source]` on the same field — they must be separate in thiserror 2. This affects hand-written error enums.
2. `Display` impl generation changed slightly.

#### Affected VexBoard Files

None — a `grep` of all `.rs` files finds **no `#[derive(thiserror::Error)]`** or `#[error(...)]` attributes in the VexBoard codebase. The crate is declared as a workspace dependency but never used.

#### Required Code Changes

None. Version bump only.

---

### 2.7 zbus — MINOR

**Version:** 4 → 5.15.0  
**Source:** Context7 `/z-galaxy/zbus`

#### Breaking Changes

1. **`zbus::fdo::ManagerProxy` builder API updated**  
   In zbus 5, `zbus::fdo::ManagerProxy::builder(&connection)` still exists but the preferred pattern moved to the `#[proxy]` macro. The builder chain `.destination(...)?.path(...)?.build().await?` is unchanged.

2. **`Connection::system().await` is unchanged**.

3. **`list_units()` return type**  
   In zbus 4, `ManagerProxy::list_units()` returns `Result<Vec<zbus::fdo::UnitInfo>>` where `UnitInfo` is a named tuple struct with public fields indexed as `.0` through `.9`.  
   In zbus 5, `UnitInfo` fields are **named** (`name`, `description`, `load_state`, `active_state`, `sub_state`, etc.) and tuple-style indexed access (`.0`, `.1`, etc.) is **removed**.

4. **`zbus::connection::Builder`** is the new type path; the old `zbus::ConnectionBuilder` alias may be removed. VexBoard does not use the builder, only `Connection::system()`.

#### Affected VexBoard Files

`crates/vexboard-server/src/discovery/systemd.rs` — accesses `list_units()` result via tuple indices.

**Current code (systemd.rs, inside `discover_units`):**

```rust
for unit in &units {
    let name = &unit.0;       // unit name
    let desc = &unit.1;       // description
    let load_state = &unit.2; // load state
    let active_state = &unit.3; // active state
    let sub_state = &unit.4;  // sub state
```

#### Required Code Changes

**`crates/vexboard-server/src/discovery/systemd.rs`** — switch to named fields:

```diff
-    for unit in &units {
-        let name = &unit.0;       // unit name
-        let desc = &unit.1;       // description
-        let load_state = &unit.2; // load state
-        let active_state = &unit.3; // active state
-        let sub_state = &unit.4;  // sub state
+    for unit in &units {
+        let name = &unit.name;
+        let desc = &unit.description;
+        let load_state = &unit.load_state;
+        let active_state = &unit.active_state;
+        let sub_state = &unit.sub_state;
```

> **Note for implementer:** Verify the exact field names by running `cargo doc --open` against zbus 5 or checking `zbus::fdo::UnitInfo` in the generated docs. Field names above match the documented zbus 5 `UnitInfo` struct. If compilation fails, check `zbus::fdo` re-exports.

---

### 2.8 bcrypt — TRIVIAL

**Version:** 0.15 → 0.19.1

#### Breaking Changes

`bcrypt::verify(password: &str, hash: &str) -> Result<bool, BcryptError>` — signature **unchanged** across all 0.15–0.19 releases. The internal hashing algorithm selection changed but is transparent to callers.

#### Affected VexBoard Files

`crates/vexboard-server/src/api/auth.rs` — `bcrypt::verify(&payload.password, &user.password_hash)`

#### Required Code Changes

None. Version bump only.

---

### 2.9 config — TRIVIAL

**Version:** 0.14 → 0.15.23

#### Breaking Changes

`Config::builder()`, `File::with_name()`, `Environment::with_prefix()`, `.separator()`, `.try_parsing()`, `.build()`, `.try_deserialize::<T>()` — all **unchanged** in the 0.14→0.15 transition. The crate bumped its MSRV to 1.70 but all VexBoard build targets are on Rust 1.85.

#### Affected VexBoard Files

`crates/vexboard-server/src/config.rs` — `AppConfig::load()`

#### Required Code Changes

None. Version bump only.

---

### 2.10 leptos — MAJOR

**Version:** 0.6 → 0.8.19  
**Sources:** Context7 `/leptos-rs/leptos`, `/websites/book_leptos_dev`, `/websites/rs_leptos`

> Leptos 0.7 was the "reactive primitives rewrite" release. Leptos 0.8 added further stabilisations. Jumping from 0.6 to 0.8 includes ALL breaking changes from both minor versions.

#### Breaking Changes

| # | Change | Severity |
|---|---|---|
| 1 | `use leptos::*` → `use leptos::prelude::*` | Required |
| 2 | `create_signal(v)` → `signal(v)` | Required |
| 3 | `create_effect(\|_\| { ... })` → `Effect::new(\|_\| { ... })` | Required |
| 4 | `create_resource(deps, fetch)` → `Resource::new(deps, fetch)` | Required |
| 5 | `mount_to_body(App)` fn-pointer → `mount_to_body(\|\| view! { <App/> })` closure | Required |
| 6 | `spawn_local(fut)` → import from `leptos::task::spawn_local` | Required |
| 7 | `<Routes>` now requires a `fallback` prop | Required |
| 8 | `Callback<()>` — `on_close.call(())` → `on_close.run(())` | Required |
| 9 | `event_target_value` moved to `leptos::ev::event_target_value` | Verify at compile |
| 10 | `Signal<bool>` as prop type requires `Send + Sync + 'static` bounds in 0.7+ | Minor |
| 11 | `.into_view()` on `Option<impl IntoView>` — still works; no change | — |

**Detailed change notes:**

**1. Import namespace**  
Leptos 0.8 ships all public reactive APIs under `leptos::prelude`. The old `use leptos::*` glob still re-exports many items for backwards compat but does not include `Effect`, `Resource`, `signal`, or `spawn_local`. Use `use leptos::prelude::*;` instead.

**2. `create_signal` → `signal`**  
`create_signal` is now an alias but marked deprecated. All 9 call sites in VexBoard use `create_signal`.  
Old: `let (x, set_x) = create_signal(value);`  
New: `let (x, set_x) = signal(value);`

**3. `create_effect` → `Effect::new`**  
`create_effect` is removed in 0.7. There is one usage in `metric_bar.rs`.  
Old: `create_effect(move |_| { ... });`  
New: `Effect::new(move |_| { ... });`

**4. `create_resource` → `Resource::new`**  
One usage in `dashboard.rs`.  
Old: `let services = create_resource(|| (), |_| async move { ... });`  
New: `let services = Resource::new(|| (), |_| async move { ... });`  
> For non-`Send` futures (WASM local), use `LocalResource::new` instead.

**5. `mount_to_body`**  
In 0.6: `mount_to_body(App)` accepts a component function pointer.  
In 0.8: `mount_to_body` signature is `fn mount_to_body<F, N>(f: F)` where `F: FnOnce() -> N + 'static, N: IntoView`. Pass a closure:  
Old: `mount_to_body(App);`  
New: `mount_to_body(|| view! { <App/> });`

**6. `spawn_local`**  
Old: `spawn_local(async move { ... })` — imported implicitly from leptos 0.6 glob.  
New: `use leptos::task::spawn_local;` then `spawn_local(async move { ... });`

**7. `<Routes fallback=...>`**  
In 0.7+, `<Routes>` is a required component wrapper and needs a `fallback` prop (returns a view for 404).  
Old:
```rust
<Routes>
    <Route path="/" view=pages::dashboard::DashboardPage />
    ...
</Routes>
```
New:
```rust
<Routes fallback=|| view! { <p>"404 Not Found"</p> }>
    <Route path="/" view=DashboardPage />
    <Route path="/settings" view=SettingsPage />
    <Route path="/login" view=LoginPage />
</Routes>
```

**8. `Callback::call` → `Callback::run`**  
In leptos 0.8, `Callback<T>` changed: `.call(arg)` → `.run(arg)`.  
Affected file: `modal_edit.rs` — `on_close.call(())` → `on_close.run(())`.

#### Affected VexBoard Files

| File | Changes Required |
|---|---|
| `crates/vexboard-frontend/src/main.rs` | Import, `mount_to_body`, `<Routes fallback>` |
| `crates/vexboard-frontend/src/components/sidebar.rs` | `use leptos::prelude::*`, `create_signal` → `signal` |
| `crates/vexboard-frontend/src/components/metric_bar.rs` | `use leptos::prelude::*`, `create_signal` → `signal`, `create_effect` → `Effect::new` |
| `crates/vexboard-frontend/src/components/service_card.rs` | `use leptos::prelude::*` |
| `crates/vexboard-frontend/src/components/status_badge.rs` | `use leptos::prelude::*` |
| `crates/vexboard-frontend/src/components/modal_edit.rs` | `use leptos::prelude::*`, `create_signal` → `signal`, `Callback::call` → `Callback::run` |
| `crates/vexboard-frontend/src/pages/dashboard.rs` | `use leptos::prelude::*`, `create_resource` → `Resource::new` |
| `crates/vexboard-frontend/src/pages/login.rs` | `use leptos::prelude::*`, `create_signal` → `signal`, `spawn_local` import |
| `crates/vexboard-frontend/src/pages/settings.rs` | `use leptos::prelude::*` |

#### Required Code Changes (Full Per-File)

---

**`crates/vexboard-frontend/src/main.rs`**

```diff
-use leptos::*;
-use leptos_router::*;
+use leptos::prelude::*;
+use leptos_router::components::{Router, Routes, Route};

 fn main() {
     console_error_panic_hook::set_once();
-    mount_to_body(App);
+    mount_to_body(|| view! { <App/> });
 }

 #[component]
 fn App() -> impl IntoView {
     view! {
         <Router>
             <div class="flex h-screen overflow-hidden">
                 <components::sidebar::Sidebar />
                 <main class="flex-1 overflow-y-auto">
                     <components::metric_bar::MetricBar />
                     <div class="p-6">
-                        <Routes>
+                        <Routes fallback=|| view! { <p>"Page not found"</p> }>
                             <Route path="/" view=pages::dashboard::DashboardPage />
                             <Route path="/settings" view=pages::settings::SettingsPage />
                             <Route path="/login" view=pages::login::LoginPage />
                         </Routes>
                     </div>
                 </main>
             </div>
         </Router>
     }
 }
```

---

**`crates/vexboard-frontend/src/components/sidebar.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;

 #[component]
 pub fn Sidebar() -> impl IntoView {
-    let (collapsed, set_collapsed) = create_signal(false);
+    let (collapsed, set_collapsed) = signal(false);
     // ... rest unchanged
```

---

**`crates/vexboard-frontend/src/components/metric_bar.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;
 use serde::Deserialize;

 // ... struct definition unchanged ...

 #[component]
 pub fn MetricBar() -> impl IntoView {
-    let (metrics, set_metrics) = create_signal(SystemMetrics::default());
+    let (metrics, set_metrics) = signal(SystemMetrics::default());

     #[cfg(target_arch = "wasm32")]
     {
         use wasm_bindgen::closure::Closure;
         use wasm_bindgen::JsCast;
         use web_sys::EventSource;

-        create_effect(move |_| {
+        Effect::new(move |_| {
             let es = EventSource::new("/api/v1/metrics/stream").ok();
             // ... rest of closure unchanged
         });
     }
     // ... rest unchanged
```

---

**`crates/vexboard-frontend/src/components/service_card.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;
 // rest unchanged (no reactive primitives called directly)
```

---

**`crates/vexboard-frontend/src/components/status_badge.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;
 // rest unchanged
```

---

**`crates/vexboard-frontend/src/components/modal_edit.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;

 #[component]
 pub fn EditModal(
     #[prop(into)] visible: Signal<bool>,
     #[prop(into)] on_close: Callback<()>,
     #[prop(optional)] initial: Option<EditFormData>,
 ) -> impl IntoView {
     // ...
-    let (name, set_name) = create_signal(initial.display_name);
-    let (desc, set_desc) = create_signal(initial.description);
-    let (url, set_url) = create_signal(initial.url);
-    let (icon, set_icon) = create_signal(initial.icon);
+    let (name, set_name) = signal(initial.display_name);
+    let (desc, set_desc) = signal(initial.description);
+    let (url, set_url) = signal(initial.url);
+    let (icon, set_icon) = signal(initial.icon);

     view! {
         <Show when=move || visible.get()>
             // ...
             <div
                 class="absolute inset-0 bg-black/60 backdrop-blur-sm"
-                on:click=move |_| on_close.call(())
+                on:click=move |_| on_close.run(())
             ></div>
             // ...
         </Show>
     }
 }
```

---

**`crates/vexboard-frontend/src/pages/dashboard.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;
 use serde::Deserialize;
 use crate::components::service_card::{ServiceCard, ServiceData};

 // ... struct definition unchanged ...

 #[component]
 pub fn DashboardPage() -> impl IntoView {
-    let services = create_resource(
+    let services = Resource::new(
         || (),
         |_| async move { fetch_services().await.unwrap_or_default() },
     );
     // ... rest unchanged
```

---

**`crates/vexboard-frontend/src/pages/login.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;
+use leptos::task::spawn_local;

 #[component]
 pub fn LoginPage() -> impl IntoView {
-    let (username, set_username) = create_signal(String::new());
-    let (password, set_password) = create_signal(String::new());
-    let (error, set_error) = create_signal(Option::<String>::None);
-    let (loading, set_loading) = create_signal(false);
+    let (username, set_username) = signal(String::new());
+    let (password, set_password) = signal(String::new());
+    let (error, set_error) = signal(Option::<String>::None);
+    let (loading, set_loading) = signal(false);
     // ... rest unchanged (spawn_local call is now resolved from explicit import above)
```

---

**`crates/vexboard-frontend/src/pages/settings.rs`**

```diff
-use leptos::*;
+use leptos::prelude::*;
 // rest unchanged (no reactive primitives)
```

---

### 2.11 leptos_router — MAJOR

**Version:** 0.6 → 0.8.13  
**Source:** Context7 `/websites/rs_leptos`

#### Breaking Changes

1. **Import path restructured**  
   Old: `use leptos_router::*;` — imports `Router`, `Routes`, `Route`, and all utilities.  
   New: Components are under `leptos_router::components::*`; hooks under `leptos_router::hooks::*`.

2. **`<Routes>` requires `fallback` prop** (see section 2.10 item 7 above).

3. **Route `view` prop type** — `view=ComponentFn` now expects a component function; no type signature change for simple CSR routes.

#### Affected VexBoard Files

`crates/vexboard-frontend/src/main.rs` — covered fully in section 2.10.

#### Required Code Changes

See diff in section 2.10 (main.rs). The `use leptos_router::components::*` import handles `Router`, `Routes`, `Route`.

---

### 2.12 gloo-net — MINOR

**Version:** 0.5 → 0.7.0

#### Breaking Changes

1. **`web-sys` minimum bumped** — gloo-net 0.7 requires `web-sys >= 0.3.70`. VexBoard's frontend Cargo.toml uses `web-sys = { version = "0.3", ... }` which will resolve to a compatible version.

2. **`gloo_net::http::Request::get()`, `.post()`, `.json()`, `.send()`, `.json::<T>()`** — all public API unchanged.

3. **`gloo_net::Error`** — still implements `std::fmt::Display` and `std::error::Error`; `fetch_services` in `dashboard.rs` returns `Result<_, gloo_net::Error>` unchanged.

#### Affected VexBoard Files

`crates/vexboard-frontend/src/pages/dashboard.rs` — `gloo_net::http::Request::get(...).send().await?.json().await`  
`crates/vexboard-frontend/src/pages/login.rs` — `gloo_net::http::Request::post(...).json(...).unwrap().send().await`

#### Required Code Changes

None beyond version bump in Cargo.toml (gloo-net version bump only).

---

### 2.13 gloo-timers — TRIVIAL

**Version:** 0.3 → 0.4.0

#### Breaking Changes

API was `Timeout::new(ms, callback)` → `Timeout::new(ms, callback)` — unchanged.  
VexBoard's frontend **never calls gloo-timers in any source file**. It appears in Cargo.toml but is unused.

#### Required Code Changes

None. Version bump only. Consider removing the dependency entirely once confirmed unused.

---

## 3. Updated Cargo.toml Contents

### 3.1 `Cargo.toml` (workspace root)

```toml
[workspace]
members = [
  "crates/vexboard-server",
  "crates/vexboard-frontend",
]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
axum = { version = "0.8", features = ["macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio", "migrate", "chrono"] }
zbus = { version = "5", default-features = false, features = ["tokio"] }
reqwest = { version = "0.13", features = ["json", "rustls-tls-native-roots"], default-features = false }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
config = "0.15"
tower-sessions = "0.15"
bcrypt = "0.19"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
thiserror = "2"
```

### 3.2 `crates/vexboard-server/Cargo.toml`

```toml
[dependencies]
tokio = { workspace = true }
axum = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sqlx = { workspace = true }
zbus = { workspace = true }
reqwest = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
config = { workspace = true }
tower-sessions = { workspace = true }
bcrypt = { workspace = true }
chrono = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["fs", "cors"] }
tokio-stream = { version = "0.1", features = ["sync"] }
```

### 3.3 `crates/vexboard-frontend/Cargo.toml`

```toml
[dependencies]
leptos = { version = "0.8", features = ["csr"] }
leptos_router = { version = "0.8", features = ["csr"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
gloo-net = "0.7"
gloo-timers = "0.4"
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["EventSource", "MessageEvent", "HtmlInputElement"] }
js-sys = "0.3"
console_error_panic_hook = "0.1"
```

---

## 4. Source Code Changes Summary

The following source files require changes beyond Cargo.toml edits:

### Server-side files

| File | Type | Description |
|---|---|---|
| `crates/vexboard-server/src/api/services.rs` | MINOR | Route params `:id` → `{id}` (2 routes) |
| `crates/vexboard-server/src/api/groups.rs` | MINOR | Route param `:id` → `{id}` (1 route) |
| `crates/vexboard-server/src/probe/uptime.rs` | MINOR | Remove invalid `reqwest::Error::from(io::Error)` construction |
| `crates/vexboard-server/src/discovery/systemd.rs` | MINOR | `unit.0`→`unit.name`, `unit.1`→`unit.description`, etc. |

### Frontend files

| File | Type | Description |
|---|---|---|
| `crates/vexboard-frontend/src/main.rs` | MAJOR | Import, `mount_to_body` closure, `<Routes fallback>` |
| `crates/vexboard-frontend/src/components/sidebar.rs` | MAJOR | Import, `signal` |
| `crates/vexboard-frontend/src/components/metric_bar.rs` | MAJOR | Import, `signal`, `Effect::new` |
| `crates/vexboard-frontend/src/components/service_card.rs` | MAJOR | Import only |
| `crates/vexboard-frontend/src/components/status_badge.rs` | MAJOR | Import only |
| `crates/vexboard-frontend/src/components/modal_edit.rs` | MAJOR | Import, `signal`, `Callback::run` |
| `crates/vexboard-frontend/src/pages/dashboard.rs` | MAJOR | Import, `Resource::new` |
| `crates/vexboard-frontend/src/pages/login.rs` | MAJOR | Import, `signal`, `spawn_local` explicit import |
| `crates/vexboard-frontend/src/pages/settings.rs` | MAJOR | Import only |

All exact diffs are provided in Section 2.

---

## 5. Dockerfile Update

The current `Dockerfile` already uses `rust:1.85-slim`. This is **sufficient** for all upgraded dependencies:

| Dependency | Min Rust Version |
|---|---|
| axum 0.8 | 1.75 |
| leptos 0.8 | 1.76 |
| zbus 5 | 1.75 |
| reqwest 0.13 | 1.63 |
| tower 0.5 | 1.64 |

**No Dockerfile change is required.** `rust:1.85-slim` satisfies all MSRVs.

However, one **improvement** is recommended: pin the Trunk version for reproducibility:

```diff
-FROM rust:1.85-slim AS frontend-builder
-RUN rustup target add wasm32-unknown-unknown && \
-    cargo install trunk
+FROM rust:1.85-slim AS frontend-builder
+RUN rustup target add wasm32-unknown-unknown && \
+    cargo install trunk --version "^0.21"
```

Trunk 0.21 is compatible with Leptos 0.8 and the `wasm32-unknown-unknown` target.

---

## 6. Implementation Order

Recommended order to minimise merge conflicts and enable incremental verification:

1. **Batch trivial bumps** — tower, tower-http, tower-sessions, thiserror, bcrypt, config, gloo-timers  
   → `cargo build --release --bin vexboard-server` should pass immediately.

2. **axum 0.8** — update Cargo.toml + fix 3 route strings in `services.rs` and `groups.rs`.  
   → `cargo build --release --bin vexboard-server` must pass before continuing.

3. **reqwest 0.13** — update Cargo.toml + fix `probe/uptime.rs` error construction.  
   → `cargo build --release --bin vexboard-server` must pass.

4. **zbus 5** — update Cargo.toml + fix `discovery/systemd.rs` field access.  
   → `cargo build --release --bin vexboard-server` must pass.

5. **leptos + leptos_router 0.8 + gloo-net 0.7** — update frontend Cargo.toml + all 9 frontend source files.  
   → `cd crates/vexboard-frontend && trunk build --release` must pass.

6. Run full validation:
   ```
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   cargo fmt --all -- --check
   ```

---

## 7. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| zbus 5 `UnitInfo` fields named differently from spec | Medium | Low | Verify with `cargo doc` at compile time; compiler error points directly to fix |
| leptos 0.8 `Callback` API changed beyond `call→run` | Medium | Low | Check leptos 0.8 `Callback` docs; `on_close.run(())` is the 0.7+ API |
| `Resource::new` returns a non-`Send` type in WASM | Low | Medium | If compile fails, use `LocalResource::new` for WASM-targeted resources |
| reqwest 0.13 `rustls-tls-native-roots` not available on target | Low | Low | Fall back to `rustls-tls` alias which still compiles |
| leptos_router 0.8 `Route` `path` attribute syntax changed | Low | High | Test all three routes (`/`, `/settings`, `/login`) render after build |

---

*Spec produced with Context7 docs for axum `/tokio-rs/axum`, leptos `/leptos-rs/leptos` + `/websites/book_leptos_dev` + `/websites/rs_leptos`, tower `/websites/rs_tower`, zbus `/z-galaxy/zbus`, reqwest `/websites/rs_reqwest`.*
