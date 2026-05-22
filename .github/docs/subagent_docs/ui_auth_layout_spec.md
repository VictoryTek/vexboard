# VexBoard — UI, Auth & Layout Improvements: Implementation Specification

**Spec Date:** 2026-05-22  
**Scope:** Authentication strategy, sidebar navigation behavior, settings cog placement, viewport layout fix  
**Status:** DRAFT — awaiting implementation

---

## 1. Current State Analysis

### 1.1 Actual Dependency Versions (authoritative — from workspace `Cargo.toml`)

> Note: The project's `copilot-instructions.md` lists outdated versions. The actual versions from `Cargo.toml` are used throughout this spec.

| Crate | Actual Version |
|---|---|
| `leptos` | **0.8** (feature `csr`) |
| `leptos_router` | **0.8** |
| `axum` | **0.8** (feature `macros`) |
| `sqlx` | **0.8** |
| `zbus` | **5** |
| `tower-http` | **0.6** |
| `tower` | **0.5** |
| `tower-sessions` | **0.15** |

---

### 1.2 Frontend — File-by-File Summary

**`crates/vexboard-frontend/src/main.rs`**
- Mounts the `App` component via `mount_to_body`.
- `App` wraps everything in `<Router>`.
- Root layout: `<div class="flex h-screen overflow-hidden">` with `<Sidebar />` and `<main class="flex-1 overflow-y-auto">`.
- Inside `<main>`: `<MetricBar />` (always rendered), then `<div class="p-6">` containing the `<Routes>`.
- Routes: `/` → `DashboardPage`, `/settings` → `SettingsPage`, `/login` → `LoginPage`.
- **No auth guard, no setup route, no first-run redirect.**

**`crates/vexboard-frontend/src/components/sidebar.rs`**
- `collapsed` signal initialized to `false` (expanded by default — conflicts with desired behavior).
- Width switches between 220px (expanded) and 60px (collapsed) via inline `style=move || format!(...)`.
- Logo text and nav item labels hidden via `(!collapsed.get()).then(|| ...)`.
- Both **Dashboard** and **Settings** nav links are inside `<nav class="flex-1 py-3 px-2 ...">`.
- Settings link is at the same hierarchy level as Dashboard — not pinned to the bottom.
- A "Collapse" toggle button is in `<div class="sidebar-footer">` with a chevron icon.
- **No hover behavior. No localStorage persistence. No SidebarMode enum.**

**`crates/vexboard-frontend/src/components/metric_bar.rs`**
- Subscribes to `/api/v1/metrics/stream` SSE as an `EventSource`.
- Rendered statically inside `<main>` — scrolls with page content (not sticky).
- CSS class `metric-bar` has `height: 52px`, `flex-shrink: 0`.

**`crates/vexboard-frontend/src/pages/dashboard.rs`**
- Fetches `/api/v1/services` via `LocalResource`.
- Renders service cards in a CSS grid (`grid-cols-1 md:grid-cols-2 lg:grid-cols-3`).
- Root element is a plain `<div>` — **no height fill, no flex-column structure**.
- Empty state uses `padding: 5rem 2rem`.

**`crates/vexboard-frontend/src/pages/settings.rs`**
- Contains only: theme toggle (dark/light via HTML class), service discovery info, About section.
- **No sidebar behavior settings exist.**

**`crates/vexboard-frontend/src/pages/login.rs`**
- Standard username/password form, posts JSON to `/api/v1/auth/login`.
- Redirects to `/` on 200 OK.
- **No awareness of first-run / setup state.**

**`crates/vexboard-frontend/style/main.css`**
- Tailwind CSS base + custom component classes.
- `.sidebar` has `transition: width 200ms ease` — CSS transition ready.
- `.sidebar-footer` has `border-top` and `padding: 0.5rem`.
- No hover rules on `.sidebar` for expansion. No `--sidebar-width` custom property.

---

### 1.3 Backend — File-by-File Summary

**`crates/vexboard-server/src/api/auth.rs`**
- `POST /api/v1/auth/login`: fetches user from DB, verifies with `bcrypt::verify`, returns JSON.
- **Session creation is a placeholder** (comment: "create a session via tower-sessions here").
- `GET /api/v1/auth/me`: always returns `401 UNAUTHORIZED` (stub).
- `POST /api/v1/auth/logout`: returns `{ "status": "logged out" }` (stub).
- **No PAM integration. No first-run detection. No setup endpoint.**

**`crates/vexboard-server/src/db/migrations/001_init.sql`**
- `users` table: `id, username, password_hash, created_at`.
- `settings` table: `key TEXT PRIMARY KEY, value TEXT NOT NULL` — available for storing flags.
- **No `setup_complete` flag column or tracking mechanism beyond user count.**

**`crates/vexboard-server/src/db/mod.rs`**
- Uses `include_str!("migrations/001_init.sql")` + `sqlx::raw_sql(...)`. Not using `sqlx::migrate!` macro, so no `.sqlx/` offline cache needed for existing queries.
- Running the same `CREATE TABLE IF NOT EXISTS` on every startup is idempotent.

**`crates/vexboard-server/src/config.rs`**
- `AppConfig` deserialized from TOML + env vars (prefix `VEXBOARD_`, separator `__`).
- `AuthConfig` has: `secret: String`, `session_ttl_hours: u64`.

**`crates/vexboard-server/src/main.rs`**
- `AppState`: `db`, `config`, `discoveries`, `metrics_tx`, `probe_tx`.
- CORS middleware: `allow_origin(Any)` — needs review for production.
- Static asset serving: falls back to `ServeDir::new("assets")` if `assets_path == "embedded"`.

**`nix/module.nix`**
- `DynamicUser = true` — **this MUST change** if PAM is needed (ephemeral user cannot read `/etc/shadow`).
- `SupplementaryGroups = [ "systemd-journal" ]`.
- No PAM service declaration.
- No `User` / `Group` fields (relies on DynamicUser).

**`nix/package.nix`**
- `buildInputs`: `openssl`, `dbus` — **no `linux-pam`**.
- Builds both backend and frontend in a single derivation.
- Frontend build runs before backend build.

**`Dockerfile`**
- Alpine 3.21 runtime.
- Backend builder: `rust:1.88-alpine` + `build-base cmake perl bash pkgconf openssl-dev`.
- No PAM libraries in the Docker build.

---

### 1.4 Current Authentication Mechanism

A bcrypt-based username/password flow against the `users` SQLite table. Session management is completely unimplemented (stubs). There is no first-run detection, no PAM integration, and no setup flow.

---

### 1.5 Current Sidebar Behavior

The sidebar starts **expanded** (`collapsed = false`), has an explicit collapse toggle button in the footer, and has **no hover-to-expand behavior**. The settings link is not pinned to the bottom — it sits in the main `<nav>` block alongside Dashboard.

---

### 1.6 Current Layout Behavior

```
<div class="flex h-screen overflow-hidden">
  <aside class="sidebar" style="width: 220px">  <!-- starts expanded -->
  <main class="flex-1 overflow-y-auto">
    <MetricBar />        <!-- 52px; inside the scroll container -->
    <div class="p-6">
      <Routes>           <!-- dashboard page, settings page, etc. -->
    </div>
  </main>
</div>
```

The `<main>` element has `overflow-y-auto`, meaning MetricBar and the entire page content scroll as one unit. Because MetricBar is NOT a sticky header, once any vertical overflow occurs, users must scroll to see both the metric bar and the service cards. The `DashboardPage` root `<div>` applies no height constraints, so the grid overflows naturally.

---

## 2. Problem Definition

### Problem 1: Authentication — Context-Dependent Strategy

When running as a NixOS/systemd service, VexBoard should authenticate users using their **OS credentials via PAM** (as Cockpit does), rather than maintaining a separate user database. This eliminates credential duplication and leverages existing NixOS user management.

When running in Docker (no PAM), VexBoard must support a **first-run setup flow**: if the `users` table is empty, the frontend must redirect to a setup page where the initial admin account is created. Subsequent logins use the standard bcrypt flow.

### Problem 2: Sidebar Collapsed by Default + Hover Expand

The sidebar starts expanded with a manual toggle. The desired UX:
- **Default state**: collapsed (icon-only, 60px wide)
- **Hover behavior**: automatically expands when mouse is over sidebar, collapses on mouse out
- **User override**: Settings page allows choosing AlwaysExpanded, AlwaysCollapsed, or HoverExpand (default)
- Preference persisted in `localStorage`

### Problem 3: Settings Cog Not Pinned to Bottom

The settings gear icon/link is mixed into the main navigation list. It should be **pinned to the bottom of the sidebar** (outside the scrollable nav area), always visible regardless of nav list length.

### Problem 4: Dashboard Content Hidden Below Viewport

The MetricBar and service cards require vertical scrolling to view. The primary dashboard content should fit within the viewport without requiring scrolling. Specifically:
- MetricBar should be a **sticky header** within the main content area (not scroll away)
- The route content area (below MetricBar) should fill remaining height and scroll **independently**

---

## 3. Proposed Architecture

### 3a. Authentication

#### Strategy: Cargo Feature Flag `pam-auth`

Two compile-time modes controlled by the Cargo feature flag `pam-auth` in `vexboard-server`:

| Mode | Feature | Auth Mechanism | First-run Setup |
|---|---|---|---|
| NixOS/systemd | `pam-auth` enabled | PAM (`libpam` / `/etc/pam.d/vexboard`) | Not needed (OS users) |
| Docker / generic | `pam-auth` disabled (default) | bcrypt DB lookup | Required if `users` table empty |

**Rationale for compile-time over runtime detection:**
- Docker's Alpine base image doesn't include `libpam` and PAM configuration wouldn't exist.
- The Dockerfile builds on Alpine and doesn't install PAM development headers.
- NixOS builds via `nix/package.nix` can explicitly pass `--features pam-auth`.
- Runtime detection via `INVOCATION_ID` env var alone is insufficient — the PAM service configuration must exist at the OS level to authenticate.

---

#### PAM Authentication Implementation (`pam-auth` feature ON)

**Crate:** `pam` v1.0 (https://crates.io/crates/pam)  
**C dependency:** `libpam` (provided by `linux-pam` on NixOS)

The `pam` crate provides a safe Rust wrapper:

```rust
// In auth.rs (inside #[cfg(feature = "pam-auth")] block)
use pam::Client;

fn authenticate_pam(username: &str, password: &str) -> Result<(), pam::PamError> {
    let mut client = Client::with_password("vexboard")?;
    client.conversation_mut().set_credentials(username, password);
    client.authenticate()?;
    Ok(())
}
```

The PAM service name `"vexboard"` maps to `/etc/pam.d/vexboard` on the system.

**Flow for `POST /api/v1/auth/login` with `pam-auth`:**
1. Receive `{ "username": "...", "password": "..." }` JSON.
2. Call `authenticate_pam(username, password)`.
3. On `Ok(())`: create session, return `{ "user": { "username": "..." } }` with 200.
4. On `Err(...)`: return `{ "error": "Invalid credentials" }` with 401.
5. **No DB lookup is needed for authentication** — PAM handles it.

**NixOS-specific PAM service configuration** (in `nix/module.nix`):
```nix
security.pam.services.vexboard = {
  # Use standard Unix authentication (pam_unix.so)
  # This allows VexBoard to authenticate OS users via /etc/shadow
};
```

**NixOS-specific user setup** — `DynamicUser = true` must be changed:
```nix
# In module.nix serviceConfig section:
DynamicUser = false;           # CHANGED: PAM requires a known user identity
User = "vexboard";
Group = "vexboard";
SupplementaryGroups = [ "shadow" "systemd-journal" ];  # shadow: read /etc/shadow
```

A dedicated NixOS user/group must be declared in the module:
```nix
users.users.vexboard = {
  isSystemUser = true;
  group = "vexboard";
  home = cfg.dataDir;
  createHome = false;
  description = "VexBoard service user";
};
users.groups.vexboard = {};
```

**`GET /api/v1/setup/status` with `pam-auth`:**  
Returns `{ "needs_setup": false, "auth_mode": "pam" }` — setup is not needed in PAM mode.

---

#### First-Run Setup Flow (Docker / `pam-auth` feature OFF)

**Backend — new endpoint `POST /api/v1/setup`:**
```
POST /api/v1/setup
Body: { "username": "...", "password": "..." }
```
- Check if `users` table is empty: `SELECT COUNT(*) FROM users`.
- If NOT empty: return `409 Conflict` (`{ "error": "Setup already completed" }`).
- Validate: username non-empty, password minimum 8 characters.
- Hash password: `bcrypt::hash(password, bcrypt::DEFAULT_COST)`.
- Insert into `users`: `INSERT INTO users (username, password_hash) VALUES (?, ?)`.
- Return `200 OK` with `{ "user": { "id": ..., "username": "..." } }`.

**Backend — new endpoint `GET /api/v1/setup/status`:**
```
GET /api/v1/setup/status
Response: { "needs_setup": bool, "auth_mode": "local" }
```
- Query `SELECT COUNT(*) FROM users` — if 0, `needs_setup: true`.

**New API route registration** in `api/mod.rs`:
```rust
.route("/api/v1/setup/status", get(setup::status))
.route("/api/v1/setup", post(setup::create_admin))
```
These routes must be registered **without authentication middleware** (they are pre-auth).

**Frontend — new file `crates/vexboard-frontend/src/pages/setup.rs`:**
- Form with username + password + confirm password fields.
- `POST /api/v1/setup` on submit.
- On success: redirect to `/login`.
- On `409 Conflict`: display "Setup already complete, please log in" and redirect.

**Frontend — new route in `main.rs`:**
```rust
<Route path=path!("/setup") view=pages::setup::SetupPage />
```

**Frontend — App-level first-run guard:**  
In `main.rs` (or a dedicated `AppShell` component), on mount:
1. Fetch `GET /api/v1/setup/status`.
2. If `needs_setup: true`, redirect browser to `/setup` (using `window.location.href = "/setup"`).
3. This check runs in an `Effect` — it only redirects if not already on `/setup` or `/login`.

---

#### Session Management (Required for Both Modes)

The current `tower-sessions` dependency exists but is not wired. Full session implementation is required for auth to actually work. This spec covers the session wire-up as part of the auth changes:

- Add `tower-sessions` middleware to the Axum router.
- `SessionManagerLayer` with an in-memory or SQLite-backed store.
- `POST /api/v1/auth/login`: on success, call `session.insert("user_id", user_id).await?`.
- `GET /api/v1/auth/me`: read `session.get::<i64>("user_id").await?`, look up user in DB, return `UserInfo`.
- `POST /api/v1/auth/logout`: call `session.flush().await?`.
- A middleware extractor on protected routes that checks session validity and returns `401` if missing.

---

### 3b. Navigation Sidebar — Collapsed Default + Hover Expand

#### `SidebarMode` Enum

New enum to be defined in `sidebar.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SidebarMode {
    HoverExpand,      // Default: collapsed at rest, expands on hover
    AlwaysExpanded,   // Always 220px wide
    AlwaysCollapsed,  // Always 60px wide
}
```

#### LocalStorage Persistence

```rust
#[cfg(target_arch = "wasm32")]
fn load_sidebar_mode_from_storage() -> SidebarMode {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("vexboard_sidebar_mode").ok().flatten())
        .map(|v| match v.as_str() {
            "always_expanded"  => SidebarMode::AlwaysExpanded,
            "always_collapsed" => SidebarMode::AlwaysCollapsed,
            _                  => SidebarMode::HoverExpand,
        })
        .unwrap_or(SidebarMode::HoverExpand)
}

#[cfg(target_arch = "wasm32")]
fn save_sidebar_mode_to_storage(mode: &SidebarMode) {
    let val = match mode {
        SidebarMode::AlwaysExpanded  => "always_expanded",
        SidebarMode::AlwaysCollapsed => "always_collapsed",
        SidebarMode::HoverExpand     => "hover_expand",
    };
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
    {
        storage.set_item("vexboard_sidebar_mode", val).ok();
    }
}
```

#### Hover Expansion Implementation

For `HoverExpand` mode, a hover signal is used:

```rust
let (hovered, set_hovered) = signal(false);

// Whether the sidebar should visually appear expanded
let is_expanded = move || match sidebar_mode.get() {
    SidebarMode::AlwaysExpanded  => true,
    SidebarMode::AlwaysCollapsed => false,
    SidebarMode::HoverExpand     => hovered.get(),
};
```

The sidebar element:
```rust
<aside
    class="sidebar"
    style=move || format!("width: {}px", if is_expanded() { 220 } else { 60 })
    on:mouseenter=move |_| set_hovered.set(true)
    on:mouseleave=move |_| set_hovered.set(false)
>
```

The CSS `transition: width 200ms ease` already exists in `.sidebar` — this is sufficient for smooth animation.

#### Settings Page Controls

The Settings page (`settings.rs`) gains a new "Navigation" section:

```
Navigation Sidebar
  Mode: [○ Hover Expand (default)]  [○ Always Expanded]  [○ Always Collapsed]
```

The `SidebarMode` signal must be accessible globally. Two approaches:
1. **Context** (Leptos context API): provide `sidebar_mode` + `set_sidebar_mode` via `provide_context` in `App`, read via `use_context` in `Sidebar` and `SettingsPage`.
2. **Module-level signal** (simpler, no prop drilling).

**Recommended: Leptos Context.** In `main.rs` `App` component:
```rust
let (sidebar_mode, set_sidebar_mode) = signal(load_sidebar_mode_from_storage());
provide_context(sidebar_mode);
provide_context(set_sidebar_mode);
```

In `Sidebar` and `SettingsPage`:
```rust
let sidebar_mode = use_context::<ReadSignal<SidebarMode>>().expect("SidebarMode context");
let set_sidebar_mode = use_context::<WriteSignal<SidebarMode>>().expect("set_sidebar_mode context");
```

---

### 3c. Settings Cog Icon at Bottom

The Settings link is removed from the main `<nav>` block and moved into the `sidebar-footer` section. The existing "Collapse" toggle button is removed (hover-to-expand replaces it; AlwaysCollapsed/AlwaysExpanded modes are in Settings).

**New sidebar structure:**
```
<aside class="sidebar ...">
  <div class="sidebar-logo">                    <!-- logo / brand -->
  <nav class="flex-1 py-3 px-2 overflow-y-auto"> <!-- Dashboard and other nav links -->
    Dashboard link
    (future nav items)
  </nav>
  <div class="sidebar-footer">                  <!-- pinned to bottom -->
    Settings gear link (active if pathname starts with /settings)
  </div>
</aside>
```

**CSS changes needed** for sidebar-footer to accommodate a nav-item-sized link:
- The existing `.sidebar-footer` has `padding: 0.5rem` and `border-top`. This is already suitable.
- The settings `<a>` uses the same `.nav-item` / `.nav-item-active` classes.

When the sidebar is collapsed (icon-only), the footer settings link shows only the gear icon (no text). When expanded, it shows gear + "Settings".

---

### 3d. Layout Fix — Viewport-Height Dashboard

#### Root Layout Change (`main.rs`)

Current `<main>` element:
```html
<main class="flex-1 overflow-y-auto">
  <MetricBar />
  <div class="p-6">...</div>
</main>
```

**Change to:**
```html
<main class="flex-1 flex flex-col overflow-hidden">
  <MetricBar />                               <!-- sticky header, never scrolls away -->
  <div class="flex-1 overflow-auto p-6">     <!-- fills remaining height, scrolls independently -->
    <Routes>...</Routes>
  </div>
</main>
```

With `overflow-hidden` on `<main>` and `flex-1 overflow-auto` on the inner `<div>`, the MetricBar stays pinned at the top of the content area. The routes content scrolls independently only when needed (e.g., many service cards).

#### MetricBar CSS

The `.metric-bar` class already has `flex-shrink: 0` — no change needed.

#### DashboardPage

The `DashboardPage` root `<div>` does not need to fill height — the outer container handles it. No changes needed to `dashboard.rs` for the basic fix.

However, for the "services fit within viewport" behavior, consider adding a minimum height to the empty-state so it fills the visible area better. This is CSS-only:
```css
/* In main.css */
.empty-state {
  min-height: calc(100vh - 56px - 52px - 3rem); /* viewport - sidebar header - metricbar - padding */
}
```
Alternatively, use `flex: 1` on the `DashboardPage` root div and make the content container `flex flex-col flex-1`.

---

## 4. File-by-File Implementation Plan

### Backend Files

#### `crates/vexboard-server/Cargo.toml`

**Changes:**
- Add `[features]` section:
  ```toml
  [features]
  default = []
  pam-auth = ["dep:pam"]
  ```
- Add conditional dependency:
  ```toml
  [target.'cfg(unix)'.dependencies]
  pam = { version = "1.0", optional = true }
  ```

#### `crates/vexboard-server/src/api/mod.rs`

**Changes:**
- Add `pub mod setup;` module declaration.
- Register new routes:
  ```rust
  .route("/api/v1/setup/status", get(setup::status))
  .route("/api/v1/setup", post(setup::create_admin))
  ```
  These routes must be added **before** any auth middleware layer.

#### `crates/vexboard-server/src/api/auth.rs`

**Changes:**

1. Wrap existing DB-based auth behind `#[cfg(not(feature = "pam-auth"))]`.
2. Add `#[cfg(feature = "pam-auth")]` branch that calls PAM:

```rust
#[cfg(feature = "pam-auth")]
async fn login(
    State(state): State<AppState>,
    mut session: tower_sessions::Session,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    use crate::pam_auth::authenticate_pam;
    match authenticate_pam(&payload.username, &payload.password) {
        Ok(()) => {
            session.insert("username", payload.username.clone()).await.ok();
            (StatusCode::OK, Json(json!({ "user": { "username": payload.username } })))
        }
        Err(_) => (StatusCode::UNAUTHORIZED, Json(json!({"error": "Invalid credentials"}))),
    }
}

#[cfg(not(feature = "pam-auth"))]
async fn login(
    State(state): State<AppState>,
    mut session: tower_sessions::Session,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    // ... existing bcrypt DB lookup logic + session.insert(...)
}
```

3. Implement `me` and `logout` handlers fully using `tower_sessions::Session`.

#### `crates/vexboard-server/src/api/setup.rs` *(NEW FILE)*

```rust
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::json;
use crate::AppState;

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

pub async fn status(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .unwrap_or(1); // fail safe: assume set up if query fails
    
    #[cfg(feature = "pam-auth")]
    let auth_mode = "pam";
    #[cfg(not(feature = "pam-auth"))]
    let auth_mode = "local";

    (StatusCode::OK, Json(json!({
        "needs_setup": count == 0,
        "auth_mode": auth_mode,
    })))
}

#[cfg(not(feature = "pam-auth"))]
pub async fn create_admin(
    State(state): State<AppState>,
    Json(payload): Json<SetupRequest>,
) -> impl axum::response::IntoResponse {
    // Guard: only allow if no users exist
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .unwrap_or(1);
    if count != 0 {
        return (StatusCode::CONFLICT, Json(json!({"error": "Setup already completed"})));
    }
    // Validate inputs
    if payload.username.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Username cannot be empty"})));
    }
    if payload.password.len() < 8 {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Password must be at least 8 characters"})));
    }
    let hash = match bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal error"}))),
    };
    match sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
        .bind(&payload.username)
        .bind(&hash)
        .execute(&state.db)
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ok"}))),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to create user"}))),
    }
}

// In PAM mode, the setup endpoint is disabled
#[cfg(feature = "pam-auth")]
pub async fn create_admin() -> impl axum::response::IntoResponse {
    (StatusCode::GONE, Json(json!({"error": "Not applicable in PAM mode"})))
}
```

#### `crates/vexboard-server/src/pam_auth.rs` *(NEW FILE, PAM feature only)*

```rust
#[cfg(feature = "pam-auth")]
pub fn authenticate_pam(username: &str, password: &str) -> Result<(), pam::PamError> {
    let mut client = pam::Client::with_password("vexboard")?;
    client.conversation_mut().set_credentials(username, password);
    client.authenticate()?;
    Ok(())
}
```

Register in `main.rs`:
```rust
#[cfg(feature = "pam-auth")]
mod pam_auth;
```

#### `crates/vexboard-server/src/main.rs`

**Changes:**
- Add `#[cfg(feature = "pam-auth")] mod pam_auth;` declaration.
- Wire up `tower-sessions` middleware:
  ```rust
  use tower_sessions::{MemoryStore, SessionManagerLayer};
  // (or SqliteStore for persistence — see dependencies section)
  
  let session_store = MemoryStore::default();
  let session_layer = SessionManagerLayer::new(session_store)
      .with_secure(false) // set true when behind HTTPS
      .with_expiry(tower_sessions::Expiry::OnInactivity(
          time::Duration::hours(config.auth.session_ttl_hours as i64)
      ));
  
  let app = api::router()
      .with_state(state)
      .layer(session_layer);
  ```

---

### Frontend Files

#### `crates/vexboard-frontend/src/main.rs`

**Changes:**
1. Add `SidebarMode` and localStorage helpers (or import from `components::sidebar`).
2. Provide `SidebarMode` context using `provide_context`.
3. Add first-run guard `Effect` to check `/api/v1/setup/status`.
4. Add `/setup` route.
5. Fix `<main>` layout from `flex-1 overflow-y-auto` to `flex-1 flex flex-col overflow-hidden`.

```rust
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use leptos::task::spawn_local;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App/> });
}

#[component]
fn App() -> impl IntoView {
    // Load sidebar mode from localStorage
    let initial_mode = {
        #[cfg(target_arch = "wasm32")]
        { components::sidebar::load_sidebar_mode_from_storage() }
        #[cfg(not(target_arch = "wasm32"))]
        { components::sidebar::SidebarMode::HoverExpand }
    };
    let (sidebar_mode, set_sidebar_mode) = signal(initial_mode);
    provide_context(sidebar_mode);
    provide_context(set_sidebar_mode);

    // First-run guard (non-PAM mode)
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(resp) = gloo_net::http::Request::get("/api/v1/setup/status")
                .send()
                .await
            {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if body["needs_setup"].as_bool().unwrap_or(false) {
                        let _ = web_sys::window()
                            .unwrap()
                            .location()
                            .set_href("/setup");
                    }
                }
            }
        });
    });

    view! {
        <Router>
            <div class="flex h-screen overflow-hidden">
                <components::sidebar::Sidebar />
                <main class="flex-1 flex flex-col overflow-hidden">
                    <components::metric_bar::MetricBar />
                    <div class="flex-1 overflow-auto p-6">
                        <Routes fallback=|| view! { <p>"Page not found"</p> }>
                            <Route path=path!("/") view=pages::dashboard::DashboardPage />
                            <Route path=path!("/settings") view=pages::settings::SettingsPage />
                            <Route path=path!("/login") view=pages::login::LoginPage />
                            <Route path=path!("/setup") view=pages::setup::SetupPage />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}
```

#### `crates/vexboard-frontend/src/components/sidebar.rs`

**Complete rewrite of the component.** Key changes:
1. Remove `collapsed` signal; replace with `SidebarMode` context.
2. Add `hovered` signal for hover-expand behavior.
3. Remove Dashboard nav items from footer; remove "Collapse" toggle button.
4. Move Settings link to `sidebar-footer`.
5. Expose `SidebarMode`, `load_sidebar_mode_from_storage`, `save_sidebar_mode_to_storage` as pub.

```rust
use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SidebarMode {
    #[default]
    HoverExpand,
    AlwaysExpanded,
    AlwaysCollapsed,
}

#[cfg(target_arch = "wasm32")]
pub fn load_sidebar_mode_from_storage() -> SidebarMode {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("vexboard_sidebar_mode").ok().flatten())
        .map(|v| match v.as_str() {
            "always_expanded"  => SidebarMode::AlwaysExpanded,
            "always_collapsed" => SidebarMode::AlwaysCollapsed,
            _                  => SidebarMode::HoverExpand,
        })
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_sidebar_mode_from_storage() -> SidebarMode {
    SidebarMode::HoverExpand
}

#[cfg(target_arch = "wasm32")]
pub fn save_sidebar_mode_to_storage(mode: &SidebarMode) {
    let val = match mode {
        SidebarMode::AlwaysExpanded  => "always_expanded",
        SidebarMode::AlwaysCollapsed => "always_collapsed",
        SidebarMode::HoverExpand     => "hover_expand",
    };
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .map(|s| s.set_item("vexboard_sidebar_mode", val).ok());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_sidebar_mode_to_storage(_mode: &SidebarMode) {}

#[component]
pub fn Sidebar() -> impl IntoView {
    let sidebar_mode = use_context::<ReadSignal<SidebarMode>>()
        .expect("SidebarMode context must be provided");
    let (hovered, set_hovered) = signal(false);
    let location = use_location();
    let pathname = location.pathname;

    let is_expanded = move || match sidebar_mode.get() {
        SidebarMode::AlwaysExpanded  => true,
        SidebarMode::AlwaysCollapsed => false,
        SidebarMode::HoverExpand     => hovered.get(),
    };

    view! {
        <aside
            class="sidebar"
            style=move || format!("width: {}px", if is_expanded() { 220 } else { 60 })
            on:mouseenter=move |_| set_hovered.set(true)
            on:mouseleave=move |_| set_hovered.set(false)
        >
            // Logo / brand
            <div class="sidebar-logo">
                <div class="sidebar-logo-icon">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="white"
                         stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="7" height="7" rx="1.5"/>
                        <rect x="14" y="3" width="7" height="7" rx="1.5"/>
                        <rect x="3" y="14" width="7" height="7" rx="1.5"/>
                        <rect x="14" y="14" width="7" height="7" rx="1.5"/>
                    </svg>
                </div>
                {move || is_expanded().then(|| view! {
                    <span class="sidebar-logo-text">"VexBoard"</span>
                })}
            </div>

            // Navigation (Dashboard only; more items can be added here)
            <nav class="flex-1 py-3 px-2 space-y-0.5 overflow-y-auto">
                <a href="/"
                   class=move || if pathname.get() == "/" { "nav-item-active" } else { "nav-item" }>
                    <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="7" height="7" rx="1"/>
                        <rect x="14" y="3" width="7" height="7" rx="1"/>
                        <rect x="3" y="14" width="7" height="7" rx="1"/>
                        <rect x="14" y="14" width="7" height="7" rx="1"/>
                    </svg>
                    {move || is_expanded().then(|| view! { <span>"Dashboard"</span> })}
                </a>
            </nav>

            // Settings cog — pinned to bottom
            <div class="sidebar-footer">
                <a href="/settings"
                   class=move || {
                       if pathname.get().starts_with("/settings") { "nav-item-active" } else { "nav-item" }
                   }
                   style="width: 100%;">
                    <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="3"/>
                        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
                    </svg>
                    {move || is_expanded().then(|| view! { <span>"Settings"</span> })}
                </a>
            </div>
        </aside>
    }
}
```

#### `crates/vexboard-frontend/src/pages/settings.rs`

**Changes:** Add a "Navigation" settings card with radio-style buttons for `SidebarMode`.

```rust
// New section added after the Appearance card:
<div class="card">
    <h2 class="text-sm font-semibold mb-3" style="color: var(--color-text-primary)">
        "Navigation Sidebar"
    </h2>
    <div class="space-y-2">
        {[
            (SidebarMode::HoverExpand,     "Hover Expand",     "Collapsed by default, expands on hover."),
            (SidebarMode::AlwaysExpanded,  "Always Expanded",  "Sidebar always shows labels."),
            (SidebarMode::AlwaysCollapsed, "Always Collapsed", "Sidebar shows icons only."),
        ].into_iter().map(|(mode, label, desc)| {
            let mode_clone = mode.clone();
            view! {
                <button
                    class=move || if sidebar_mode.get() == mode_clone { "nav-item nav-item-active" } else { "nav-item" }
                    style="width: 100%; text-align: left; padding: 0.625rem 0.75rem;"
                    on:click=move |_| {
                        set_sidebar_mode.set(mode.clone());
                        #[cfg(target_arch = "wasm32")]
                        crate::components::sidebar::save_sidebar_mode_to_storage(&mode);
                    }
                >
                    <div>
                        <p class="text-sm font-medium">{label}</p>
                        <p class="text-xs mt-0.5" style="color: var(--color-text-muted)">{desc}</p>
                    </div>
                </button>
            }
        }).collect_view()}
    </div>
</div>
```

#### `crates/vexboard-frontend/src/pages/setup.rs` *(NEW FILE)*

```rust
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn SetupPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (confirm, set_confirm)   = signal(String::new());
    let (error, set_error)       = signal(Option::<String>::None);
    let (loading, set_loading)   = signal(false);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let u = username.get();
        let p = password.get();
        let c = confirm.get();
        if p != c {
            set_error.set(Some("Passwords do not match".into()));
            return;
        }
        if p.len() < 8 {
            set_error.set(Some("Password must be at least 8 characters".into()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            let result = gloo_net::http::Request::post("/api/v1/setup")
                .json(&serde_json::json!({ "username": u, "password": p }))
                .unwrap()
                .send()
                .await;
            set_loading.set(false);
            match result {
                Ok(resp) if resp.ok() => {
                    #[cfg(target_arch = "wasm32")]
                    web_sys::window().unwrap().location().set_href("/login").ok();
                }
                Ok(resp) if resp.status() == 409 => {
                    set_error.set(Some("Setup already completed — please log in.".into()));
                    #[cfg(target_arch = "wasm32")]
                    web_sys::window().unwrap().location().set_href("/login").ok();
                }
                Ok(_) => set_error.set(Some("Setup failed — please try again.".into())),
                Err(e) => set_error.set(Some(format!("Network error: {e}"))),
            }
        });
    };

    view! {
        <div class="flex flex-col items-center justify-center"
             style="min-height: 80vh; gap: 1.5rem">
            <div class="text-center">
                <h1 class="text-xl font-semibold tracking-tight">"Welcome to VexBoard"</h1>
                <p class="text-xs mt-1" style="color: var(--color-text-muted)">
                    "Create your admin account to get started."
                </p>
            </div>
            <div class="card" style="width: 100%; max-width: 360px;">
                {move || error.get().map(|e| view! {
                    <div class="mb-4 px-3 py-2.5 rounded-lg text-xs"
                         style="background: var(--color-danger-dim); color: var(--color-danger); border: 1px solid rgba(239,68,68,0.2)">
                        {e}
                    </div>
                })}
                <form on:submit=on_submit class="space-y-4">
                    <div>
                        <label class="form-label">"Username"</label>
                        <input type="text" class="form-input" required=true
                               prop:value=move || username.get()
                               on:input=move |ev| set_username.set(event_target_value(&ev)) />
                    </div>
                    <div>
                        <label class="form-label">"Password"</label>
                        <input type="password" class="form-input" required=true
                               prop:value=move || password.get()
                               on:input=move |ev| set_password.set(event_target_value(&ev)) />
                    </div>
                    <div>
                        <label class="form-label">"Confirm Password"</label>
                        <input type="password" class="form-input" required=true
                               prop:value=move || confirm.get()
                               on:input=move |ev| set_confirm.set(event_target_value(&ev)) />
                    </div>
                    <button type="submit" class="btn-primary"
                            style="width: 100%; justify-content: center;"
                            disabled=move || loading.get()>
                        {move || if loading.get() { "Creating account…" } else { "Create Admin Account" }}
                    </button>
                </form>
            </div>
        </div>
    }
}
```

#### `crates/vexboard-frontend/src/pages/mod.rs`

**Change:** Add `pub mod setup;`.

#### `crates/vexboard-frontend/style/main.css`

**No changes strictly required** for the core layout fix (handled by Tailwind classes in `main.rs`). However, add a CSS variable for sidebar transitions and a hover-safe-zone improvement:

```css
/* Optional: ensure sidebar hover area doesn't feel too narrow in collapsed state */
.sidebar {
  /* existing styles preserved */
  /* Add: min-width for collapsed icon display */
  min-width: 60px;
}
```

The `transition: width 200ms ease` already present handles smooth expand/collapse animation.

---

### NixOS Files

#### `nix/module.nix`

**Changes:**
1. Add `pam` feature to the build / `pkgs.vexboard` (see `package.nix`).
2. Remove `DynamicUser = true`.
3. Add `User`, `Group`, `SupplementaryGroups`.
4. Declare user and group.
5. Add PAM service declaration.

```nix
# In config = lib.mkIf cfg.enable { ... } section, add:

users.users.vexboard = {
  isSystemUser = true;
  group = "vexboard";
  home = cfg.dataDir;
  description = "VexBoard service user";
};
users.groups.vexboard = {};

security.pam.services.vexboard = {};  # Creates /etc/pam.d/vexboard with standard Unix auth

# In systemd.services.vexboard.serviceConfig, change:
# DynamicUser = true;        <- REMOVE
DynamicUser = false;          # ADD
User = "vexboard";            # ADD
Group = "vexboard";           # ADD
SupplementaryGroups = [ "shadow" "systemd-journal" ];  # ADD shadow for /etc/shadow access
```

#### `nix/package.nix`

**Changes:**
1. Add `linux-pam` to `buildInputs`.
2. Pass `--features pam-auth` to the backend build command.

```nix
buildInputs = [
  openssl
  dbus
  linux-pam   # ADD: required for pam crate linkage
];

buildPhase = ''
  cd crates/vexboard-frontend
  trunk build --release
  cd ../..
  cargo build --release --bin vexboard-server --features pam-auth  # ADD --features pam-auth
'';
```

---

## 5. Dependencies

### `crates/vexboard-server/Cargo.toml` — New Dependencies

| Crate | Version | Feature | Purpose | Conditional |
|---|---|---|---|---|
| `pam` | `1.0` | — | PAM authentication via libpam | `optional = true`, only with `pam-auth` feature |
| `time` | `0.3` | `"macros"` | Required by `tower-sessions` for expiry duration | Only if not already transitive |

**Workspace `Cargo.toml`** — consider adding `time` as a workspace dep if not already present:
```toml
time = { version = "0.3", features = ["macros"] }
```

Check `Cargo.lock` first — `time` is likely already a transitive dependency of `tower-sessions`.

### `crates/vexboard-frontend/Cargo.toml` — No New Dependencies

`serde_json` is already present and used in `login.rs`. `web-sys` `localStorage` API is available via the existing `web-sys` dependency, but `Window.local_storage()` requires the `Window` web-sys feature which may already be included. Verify `web-sys` features include `Storage` and `Window`:

Current features: `["EventSource", "MessageEvent", "HtmlInputElement"]`

**Add** `"Window"`, `"Storage"` to `web-sys` features in `crates/vexboard-frontend/Cargo.toml`:
```toml
web-sys = { version = "0.3", features = [
    "EventSource",
    "MessageEvent",
    "HtmlInputElement",
    "Window",     # ADD — needed for window().local_storage()
    "Storage",    # ADD — needed for localStorage.get_item / set_item
]}
```

---

## 6. Configuration Changes

### `config/default.toml`

**No new keys required** for the core changes. However, document the deployment mode:

```toml
[auth]
# Session secret — override with VEXBOARD_AUTH_SECRET env var in production
secret = "change-me-in-production"
session_ttl_hours = 168  # 7 days
# Note: when built with --features pam-auth (NixOS builds), VexBoard authenticates
# users via PAM (Linux system accounts). The users table and setup flow are not used.
```

### `nix/module.nix`

New NixOS options to consider adding:

```nix
allowedUsers = lib.mkOption {
  type = lib.types.listOf lib.types.str;
  default = [];
  description = ''
    List of Linux usernames allowed to log in to VexBoard.
    If empty, any valid PAM authentication is accepted.
    Implementation via PAM pam_listfile.so or application-level allowlist.
  '';
};
```

This is optional for the initial implementation but important for hardening — without it, any valid Linux user can log in.

---

## 7. Risks and Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| **PAM `shadow` group access**: `vexboard` user in `shadow` group can read all password hashes from `/etc/shadow`. If VexBoard is compromised, this is a privilege escalation path. | High | Consider using `pam_unix.so` with a dedicated PAM helper (like `unix_chkpwd`) that is setuid root. NixOS configures this automatically; ensure `security.pam.services.vexboard` uses the default `pam_unix` stack. Alternatively, restrict allowed users via `pam_listfile.so`. |
| **DynamicUser removal**: Changes NixOS security posture (less isolation). | Medium | The dedicated `vexboard` user is still a system user with minimal privileges. `ProtectSystem = "strict"` and `PrivateTmp = true` remain. The risk is acceptable given the PAM requirement. |
| **Setup endpoint race condition**: Two simultaneous requests to `POST /api/v1/setup` could both pass the "no users" check. | Low | The `INSERT INTO users (username, ...)` will fail with `UNIQUE constraint failed` for the second request due to the `UNIQUE` constraint on `username`. Handle the DB error and return a `409 Conflict`. Additionally, SQLite's WAL mode serializes writes. |
| **First-run check on every page load**: The `Effect` in `App` fetches `/api/v1/setup/status` on every mount. Slight performance overhead. | Low | The endpoint is a single `COUNT(*)` SQLite query — negligible cost. Cache the result in a Leptos `Resource` with no refetch, or use `OnceCell`. |
| **`SidebarMode` context missing**: If `use_context::<ReadSignal<SidebarMode>>()` is called without `provide_context` in parent, it panics at runtime. | Medium | Use `.expect("...")` with a clear message. Ensure `provide_context` is always called in `App` before the `Router`. Add a test or assertion. |
| **Leptos 0.8 `provide_context` / `use_context` API**: Verify the exact API hasn't changed between 0.6 → 0.8. The project uses 0.8 (not 0.6 as stated in `copilot-instructions.md`). | Medium | In Leptos 0.8, `provide_context` and `use_context` are unchanged from 0.6. `ReadSignal<T>` and `WriteSignal<T>` are separate types. Signals created with `signal()` return `(ReadSignal<T>, WriteSignal<T>)`. The patterns in this spec are compatible with Leptos 0.8 CSR. |
| **`web-sys` `Window` feature**: Calling `web_sys::window()` without the `Window` feature enabled in `Cargo.toml` will fail to compile. | Medium | Explicitly add `"Window"` and `"Storage"` to `web-sys` features. Already identified in Section 5. |
| **Alpine Docker image + PAM**: If someone passes `--features pam-auth` to the Docker build (accidentally), `libpam` won't be installed in Alpine, causing link failure. | Low | Document in `Dockerfile` that PAM is not supported. The `default` feature set does NOT include `pam-auth`. The Docker build must not pass `--features pam-auth`. Consider adding a comment to the Dockerfile. |
| **Tower-sessions `MemoryStore` is in-process only**: Sessions are lost on server restart. | Medium | For production NixOS deployments, use `tower-sessions-sqlx-store` (SQLite-backed sessions). For the initial implementation, `MemoryStore` is acceptable. Add a `TODO` comment. |
| **CORS is `allow_origin(Any)` in production**: The current CORS config allows all origins. | High | Restrict CORS in production builds or via config. Add `VEXBOARD_SERVER__ALLOWED_ORIGIN` config key. This is a pre-existing security issue — flag it but do not block this feature set on it. |

---

## 8. Implementation Order

Recommended sequence for the implementation subagent:

1. **Backend first:**
   a. Add `pam-auth` feature flag to `vexboard-server/Cargo.toml`
   b. Create `api/setup.rs` (status + create_admin endpoints)
   c. Update `api/mod.rs` to register setup routes
   d. Update `auth.rs` with `#[cfg]` branches and wire session insertion
   e. Update `main.rs` to wire `tower-sessions` middleware and declare `pam_auth` module
   f. Create `pam_auth.rs` (PAM feature module)

2. **Frontend second:**
   a. Update `web-sys` features in `Cargo.toml`
   b. Create `pages/setup.rs`
   c. Update `pages/mod.rs`
   d. Update `components/sidebar.rs` (full rewrite)
   e. Update `pages/settings.rs` (add Navigation section)
   f. Update `main.rs` (context, guard, layout, route)

3. **NixOS third:**
   a. Update `nix/module.nix` (user, group, PAM, DynamicUser)
   b. Update `nix/package.nix` (linux-pam buildInput, --features pam-auth)

---

## Summary of All Files Modified or Created

| File | Action | Description |
|---|---|---|
| `crates/vexboard-server/Cargo.toml` | Modify | Add `pam-auth` feature, `pam` optional dep |
| `crates/vexboard-server/src/main.rs` | Modify | Wire tower-sessions, declare pam_auth module |
| `crates/vexboard-server/src/api/mod.rs` | Modify | Register setup routes |
| `crates/vexboard-server/src/api/auth.rs` | Modify | `#[cfg]` PAM/DB auth branches, wire session |
| `crates/vexboard-server/src/api/setup.rs` | **CREATE** | Setup status + create_admin endpoints |
| `crates/vexboard-server/src/pam_auth.rs` | **CREATE** | PAM feature wrapper |
| `crates/vexboard-frontend/Cargo.toml` | Modify | Add `Window`, `Storage` to web-sys features |
| `crates/vexboard-frontend/src/main.rs` | Modify | Context, guard, layout fix, setup route |
| `crates/vexboard-frontend/src/components/sidebar.rs` | Modify | SidebarMode, hover, settings-at-bottom |
| `crates/vexboard-frontend/src/pages/mod.rs` | Modify | Add `pub mod setup;` |
| `crates/vexboard-frontend/src/pages/settings.rs` | Modify | Add Navigation sidebar mode section |
| `crates/vexboard-frontend/src/pages/setup.rs` | **CREATE** | First-run admin creation page |
| `nix/module.nix` | Modify | User/group, PAM service, remove DynamicUser |
| `nix/package.nix` | Modify | Add linux-pam, pass --features pam-auth |
| `config/default.toml` | Modify (optional) | Add clarifying comments for auth section |
