# Feature Specification: User Account Menu (Top-Right UI)

**Feature:** User account info panel with per-mode settings  
**Date:** 2026-05-30  
**Status:** DRAFT

---

## 1. Current State Analysis

### 1.1 Authentication Architecture

VexBoard uses **two mutually exclusive auth modes**, selected at compile time via the `pam-auth` Cargo feature flag:

| Mode | Feature flag | Auth mechanism | Password storage |
|------|-------------|----------------|-----------------|
| **Local** (Docker) | *(default, no flag)* | SQLite `users` table | `bcrypt` (crate `bcrypt = "0.19"`) |
| **PAM** (Nix flake) | `pam-auth` (`unix` target) | Linux PAM (`pam_sys`) | OS account |

The compile-time guard pattern used throughout the server codebase is:

```rust
#[cfg(all(unix, feature = "pam-auth"))]   // PAM path
#[cfg(not(all(unix, feature = "pam-auth")))] // local path
```

The `setup.rs` API already exposes `auth_mode` at `GET /api/v1/setup/status` returning `{"auth_mode": "pam"}` or `{"auth_mode": "local"}`. This is the established runtime pattern for communicating the mode to the frontend.

### 1.2 Session Management

- **No JWT** — sessions are cookie-based via `tower-sessions` (`MemoryStore`).
- Session key: `"username"` (a `String`).
- After credential change: flush session → client is unauthenticated → redirect to `/login`.

### 1.3 Frontend Layout Structure

The `MainLayout` component (`crates/vexboard-frontend/src/main.rs`) renders:

```
<div style="display:flex; height:100vh; overflow:hidden;">
  <Sidebar />                                  ← 60px / 220px, left
  <main style="flex:1; display:flex; flex-direction:column; overflow:hidden; min-width:0;">
    <MetricBar />                              ← height:52px, top strip
    <div style="flex:1; overflow:auto; padding:1.5rem;">
      <Outlet />                               ← page content
    </div>
  </main>
</div>
```

**MetricBar** (`src/components/metric_bar.rs`): A `div.metric-bar` with `display:flex; align-items:center; height:52px; padding:0 1.25rem; gap:0.25rem`. It currently shows CPU / RAM / NET / DISK metrics left-aligned via SSE. There is no user identity information anywhere in the layout.

### 1.4 Existing `/api/v1/auth/me` Endpoint

Located in `crates/vexboard-server/src/api/auth.rs`:

```rust
async fn me(session: Session) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(username)) => (StatusCode::OK, Json(json!({ "user": { "username": username } }))),
        _ => (StatusCode::UNAUTHORIZED, Json(json!({"error": "Not authenticated"}))),
    }
}
```

**Gap**: does not return `auth_mode`. The frontend cannot distinguish deployment modes at runtime.

### 1.5 Existing Frontend Auth Usage

- The frontend does not currently call `/api/v1/auth/me` post-login; username is not displayed anywhere.
- Logout is not exposed in the UI — only the `/api/v1/auth/logout` backend endpoint exists.
- `UserInfo` model in `db/models.rs` has `id: i64, username: String` but no `auth_mode` field.

### 1.6 Cargo / Leptos Version

The frontend uses **Leptos 0.8 CSR** (`leptos = { version = "0.8", features = ["csr"] }`). Not 0.6. All component patterns must follow Leptos 0.8 conventions as observed in the existing code.

### 1.7 Password Hashing

`bcrypt` crate version `0.19` is already a workspace dependency and used in `setup.rs` and `auth.rs`. No new hashing dependency is required.

---

## 2. Problem Definition

### 2.1 What Needs to Be Added

A **user account menu** anchored to the top-right corner of the application shell that:

1. Displays the authenticated user's username (or initials avatar).
2. Opens a dropdown on click with:
   - Username display (read-only header)
   - "Account Settings" button → opens a modal
   - "Logout" button → calls `/api/v1/auth/logout` and redirects to `/login`
3. The Account Settings modal:
   - **Docker/local mode**: allows changing username and/or password (requires current password verification).
   - **Nix/PAM mode**: shows a read-only informational message that account settings are managed by the OS; no form fields for credentials are rendered.

### 2.2 Deployment Mode Behaviors

| Behavior | Local (Docker) | PAM (Nix) |
|----------|---------------|-----------|
| Show username in menu | ✅ | ✅ |
| Logout button | ✅ | ✅ |
| "Account Settings" opens modal | ✅ | ✅ (read-only info) |
| Username change form | ✅ | ❌ |
| Password change form | ✅ | ❌ |
| PATCH /api/v1/user/me endpoint functional | ✅ | ❌ (405) |

### 2.3 How to Distinguish Docker vs Nix at Runtime

The approach already established by `setup.rs` is canonical: return `auth_mode: "local"` or `auth_mode: "pam"` from the API, controlled by `#[cfg]` attributes. Extending the existing `/api/v1/auth/me` endpoint to include `auth_mode` is the minimal, consistent approach — the frontend already calls this endpoint at or shortly after login to identify the current user. **No new config key or environment variable is needed.**

---

## 3. Proposed Solution Architecture

### 3.1 Deployment Mode Detection

**Approach:** Extend `GET /api/v1/auth/me` to return `auth_mode` in the response body. This avoids a separate endpoint and gives the frontend both the username and the mode in a single request.

New response shape:
```json
{
  "user": {
    "username": "admin",
    "auth_mode": "local"
  }
}
```

Or for PAM builds:
```json
{
  "user": {
    "username": "alice",
    "auth_mode": "pam"
  }
}
```

`auth_mode` is a compile-time constant injected via `#[cfg]`:

```rust
// In auth.rs me() handler:
#[cfg(all(unix, feature = "pam-auth"))]
const AUTH_MODE: &str = "pam";
#[cfg(not(all(unix, feature = "pam-auth")))]
const AUTH_MODE: &str = "local";

async fn me(session: Session) -> impl IntoResponse {
    match session.get::<String>("username").await {
        Ok(Some(username)) => (
            StatusCode::OK,
            Json(json!({ "user": { "username": username, "auth_mode": AUTH_MODE } })),
        ),
        _ => (StatusCode::UNAUTHORIZED, Json(json!({"error": "Not authenticated"}))),
    }
}
```

**No changes to `config/default.toml` or `AppConfig` are required.**

### 3.2 Backend Changes

#### 3.2.1 Modify `GET /api/v1/auth/me`

File: `crates/vexboard-server/src/api/auth.rs`

- Add `AUTH_MODE` compile-time constant (two `#[cfg]` blocks).
- Extend JSON response to include `"auth_mode": AUTH_MODE`.

#### 3.2.2 New `PATCH /api/v1/user/me` Endpoint

File: `crates/vexboard-server/src/api/auth.rs` (added to same module and router)

Route registered as:

```rust
.route("/me", patch(update_me))
```

**Request body** (`UpdateMeRequest`):

```rust
#[derive(Deserialize)]
struct UpdateMeRequest {
    current_password: String,
    new_username: Option<String>,
    new_password: Option<String>,
}
```

**Local (non-PAM) implementation:**

```rust
#[cfg(not(all(unix, feature = "pam-auth")))]
async fn update_me(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<UpdateMeRequest>,
) -> impl IntoResponse {
    // 1. Authenticate session
    let username = match session.get::<String>("username").await {
        Ok(Some(u)) => u,
        _ => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Not authenticated"}))),
    };

    // 2. Fetch user record
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, created_at FROM users WHERE username = ?"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await;

    let user = match user {
        Ok(Some(u)) => u,
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"}))),
    };

    // 3. Verify current password
    let valid = bcrypt::verify(&payload.current_password, &user.password_hash).unwrap_or(false);
    if !valid {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "Incorrect current password"})));
    }

    // 4. Validate new values
    let new_username = payload.new_username.as_deref().map(str::trim);
    let new_password = payload.new_password.as_deref();

    if let Some(u) = new_username {
        if u.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Username cannot be empty"})));
        }
    }
    if let Some(p) = new_password {
        if p.len() < 8 {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Password must be at least 8 characters"})));
        }
    }

    // 5. Apply changes
    if let Some(u) = new_username {
        let res = sqlx::query("UPDATE users SET username = ? WHERE id = ?")
            .bind(u)
            .bind(user.id)
            .execute(&state.db)
            .await;
        if res.is_err() {
            return (StatusCode::CONFLICT, Json(json!({"error": "Username already taken"})));
        }
    }

    if let Some(p) = new_password {
        let hash = match bcrypt::hash(p, bcrypt::DEFAULT_COST) {
            Ok(h) => h,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Hash error"}))),
        };
        let res = sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(user.id)
            .execute(&state.db)
            .await;
        if res.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"})));
        }
    }

    // 6. Flush session — force re-login
    session.flush().await.ok();

    (StatusCode::OK, Json(json!({"status": "ok", "reauth_required": true})))
}
```

**PAM stub (returns 405):**

```rust
#[cfg(all(unix, feature = "pam-auth"))]
async fn update_me() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({"error": "Account changes are not supported in PAM mode"})),
    )
}
```

#### 3.2.3 Router Update

File: `crates/vexboard-server/src/api/auth.rs`

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me).patch(update_me))  // add patch
}
```

No new file or module needed. The `patch` method is added to the same route as `get`.

#### 3.2.4 `UpdateMeRequest` Model

Add `UpdateMeRequest` struct directly in `auth.rs` (it is small and endpoint-local). No changes to `db/models.rs` needed; `User` is already imported in that file.

### 3.3 Frontend Changes

#### 3.3.1 New `UserMenu` Component

**File:** `crates/vexboard-frontend/src/components/user_menu.rs`

**Responsibilities:**
- Fetch `GET /api/v1/auth/me` once on mount via `LocalResource`.
- Maintain a boolean `open` signal for dropdown visibility.
- Maintain a boolean `show_settings_modal` signal.
- Render an avatar button (user initials or user icon) in the top-right.
- On click: toggle `open` dropdown.
- Dropdown contains: username (non-interactive label), "Account Settings" button, separator, "Logout" button.
- "Logout" calls `/api/v1/auth/logout` then navigates to `/login`.
- "Account Settings" sets `show_settings_modal = true` and closes dropdown.
- `AccountSettingsModal` is rendered inline within `UserMenu`; it is shown/hidden via a `Show` component.

**`UserInfo` struct (local to component file):**

```rust
#[derive(Debug, Clone, serde::Deserialize, Default)]
struct UserInfo {
    username: String,
    auth_mode: String,  // "local" or "pam"
}
```

**Leptos 0.8 patterns to use:**

```rust
// Async data fetch
let user_info = LocalResource::new(|| async move { fetch_me().await.unwrap_or_default() });

// Toggle signals
let (open, set_open) = signal(false);
let (show_settings, set_show_settings) = signal(false);

// Derived: initials from username
let initials = move || {
    user_info.get()
        .map(|u| u.username.chars().next().unwrap_or('?').to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
};
```

**Close-on-outside-click**: Implement via a document-level `click` listener attached in `Effect::new`. When the dropdown is open, listen for a click on the document and close. Use `event.stop_propagation()` on the menu element to prevent self-close.

**Account Settings Modal conditional rendering:**

```rust
{move || {
    let is_pam = user_info.get()
        .map(|u| u.auth_mode == "pam")
        .unwrap_or(false);

    if is_pam {
        Either::Left(view! {
            <p class="text-xs" style="color: var(--color-text-muted); padding: 0.5rem 0;">
                "Account settings are managed by the operating system. \
                 Use system tools to change your password."
            </p>
        })
    } else {
        Either::Right(view! { <CredentialChangeForm ... /> })
    }
}}
```

**`CredentialChangeForm` sub-component (within same file):**

Fields:
1. New Username (optional, pre-filled with current username)
2. New Password (optional)
3. Confirm New Password (optional, validated to match New Password)
4. Current Password (required)

On submit:
- Validate fields client-side (non-empty new_username if changed, new_password >= 8 chars, passwords match).
- `POST`-style → actually `PATCH /api/v1/auth/me` with `Content-Type: application/json`.
- On success (`reauth_required: true`): navigate to `/login`.
- On error: display error message inline in the modal.

#### 3.3.2 Component Registration

File: `crates/vexboard-frontend/src/components/mod.rs`

Add:
```rust
pub mod user_menu;
```

#### 3.3.3 Placement in Layout

**Where:** Inside `MetricBar` component (`src/components/metric_bar.rs`), pushed to the far right of the bar.

**How:** Wrap all existing metric items in a `<div style="display:flex; align-items:center; gap:0.25rem; flex:1;">` (moving gap from `.metric-bar` to the inner wrapper), then append `<components::user_menu::UserMenu />` as a sibling, which is naturally pushed right since the wrapper uses `flex:1`. Alternatively and more minimally: add `<UserMenu />` inside the `.metric-bar` div with `margin-left: auto` applied to its wrapper. This requires the least change to existing layout.

**Exact DOM modification** in `metric_bar.rs`:

```rust
view! {
    <div class="metric-bar">
        // ... existing metric items (unchanged) ...

        // Spacer + User menu — pushed to right edge
        <div style="margin-left: auto; display:flex; align-items:center;">
            <components::user_menu::UserMenu />
        </div>
    </div>
}
```

The `metric-bar` div does NOT need `justify-content: space-between` because `margin-left: auto` on the user menu wrapper achieves the right-alignment without reordering existing items.

Note: `UserMenu` must be imported via `use crate::components::user_menu::UserMenu;` at the top of `metric_bar.rs`, or referenced as `crate::components::user_menu::UserMenu` inline.

#### 3.3.4 Context Propagation (Optional Optimization)

If `UserMenu` in `MetricBar` fetches `/api/v1/auth/me` independently, there is no need to pass data via context. The resource is fetched once on mount and cached by Leptos's `LocalResource`. This is sufficient and consistent with how `DashboardPage` uses `LocalResource::new(|| async move { fetch_services().await... })`.

No additional context provision in `MainLayout` is required.

### 3.4 CSS Changes

**File:** `crates/vexboard-frontend/style/main.css`

Add to the `@layer components` block:

```css
/* ── User Menu ── */
.user-menu-trigger {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.3rem 0.5rem;
  border-radius: 0.5rem;
  border: none;
  background: transparent;
  cursor: pointer;
  transition: background-color 150ms;
  color: var(--color-text-secondary);
}

.user-menu-trigger:hover {
  background-color: var(--color-bg-hover);
  color: var(--color-text-primary);
}

.user-menu-avatar {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  background: linear-gradient(135deg, #3b82f6 0%, #6366f1 100%);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.6875rem;
  font-weight: 700;
  color: white;
  flex-shrink: 0;
  user-select: none;
}

.user-menu-username {
  font-size: 0.8125rem;
  font-weight: 500;
  white-space: nowrap;
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.user-menu-dropdown {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  min-width: 200px;
  background-color: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: 0.75rem;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  z-index: 100;
  overflow: hidden;
}

.user-menu-dropdown-header {
  padding: 0.875rem 1rem 0.75rem;
  border-bottom: 1px solid var(--color-border);
}

.user-menu-dropdown-section {
  padding: 0.375rem;
}

.user-menu-item {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  width: 100%;
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--color-text-secondary);
  background: transparent;
  border: none;
  cursor: pointer;
  transition: background-color 150ms, color 150ms;
  text-align: left;
}

.user-menu-item:hover {
  background-color: var(--color-bg-hover);
  color: var(--color-text-primary);
}

.user-menu-item-danger {
  color: var(--color-danger);
}

.user-menu-item-danger:hover {
  background-color: var(--color-danger-dim);
  color: var(--color-danger);
}

.user-menu-sep {
  height: 1px;
  background-color: var(--color-border);
  margin: 0;
}
```

The `.user-menu-dropdown` uses `position: absolute` with the trigger wrapper set to `position: relative`. No changes to `.metric-bar` CSS class are needed.

---

## 4. Implementation Steps (Ordered)

### Phase A — Backend

1. **`crates/vexboard-server/src/api/auth.rs`**
   - Add `UpdateMeRequest` struct (derives `Deserialize`).
   - Add `AUTH_MODE` compile-time constant (two `#[cfg]` blocks).
   - Modify `me()` handler to include `"auth_mode": AUTH_MODE` in JSON response.
   - Add `update_me()` handler — two `#[cfg]` variants (local and PAM stub).
   - Update `router()` to add `.patch(update_me)` to the `/me` route.

   No other backend files need modification.

### Phase B — Frontend Component

2. **`crates/vexboard-frontend/src/components/user_menu.rs`** *(new file)*
   - Define `UserInfo` struct (Deserialize, Default).
   - Define `fetch_me()` async fn calling `GET /api/v1/auth/me`.
   - Define `CredentialChangeForm` component (local-mode only form).
   - Define `AccountSettingsModal` component (wraps `CredentialChangeForm` or PAM message inside `EditModal`-style overlay panel).
   - Define `UserMenu` component (main export):
     - `LocalResource` for user info.
     - `(open, set_open)` signal.
     - `(show_settings, set_show_settings)` signal.
     - Avatar trigger button (`.user-menu-trigger`).
     - `Show` dropdown (`.user-menu-dropdown`).
     - `Show` account settings modal.

3. **`crates/vexboard-frontend/src/components/mod.rs`**
   - Add `pub mod user_menu;`

4. **`crates/vexboard-frontend/src/components/metric_bar.rs`**
   - Add `use crate::components::user_menu::UserMenu;` (or full path inline).
   - Add `<div style="margin-left: auto; display:flex; align-items:center; position:relative;">` containing `<UserMenu />` inside the `.metric-bar` div, after the existing disk metric item.

### Phase C — CSS

5. **`crates/vexboard-frontend/style/main.css`**
   - Append user menu CSS rules to the `@layer components` block (`.user-menu-trigger`, `.user-menu-avatar`, `.user-menu-username`, `.user-menu-dropdown`, `.user-menu-dropdown-header`, `.user-menu-dropdown-section`, `.user-menu-item`, `.user-menu-item-danger`, `.user-menu-sep`).

---

## 5. Dependencies

### New Cargo Dependencies
**None required.** All needed crates are already present:

| Crate | Already in workspace | Usage |
|-------|---------------------|-------|
| `bcrypt = "0.19"` | ✅ workspace dep | Password verify + hash in `update_me` |
| `tower-sessions = "0.15"` | ✅ workspace dep | Session flush after credential change |
| `axum = "0.8"` | ✅ workspace dep | `patch` method on router |
| `serde` / `serde_json` | ✅ both crates | Request/response types |
| `gloo-net = "0.7"` | ✅ frontend dep | `PATCH` request from browser |

### Leptos Features

No additional `web-sys` features are needed. The existing feature list in `vexboard-frontend/Cargo.toml` already includes `Window`, `Location`, `Storage`, `HtmlElement`, and `HtmlInputElement` — all needed for the user menu and navigation.

---

## 6. Security Considerations

### 6.1 Current Password Verification Before Any Change
- `PATCH /api/v1/auth/me` requires `current_password` in every request.
- Verified with `bcrypt::verify()` against the stored hash before any database write.
- No partial state is written if verification fails (atomicity per-field: first username, then password).

### 6.2 Session Invalidation After Credential Change
- After a successful username or password change, `session.flush().await` is called.
- The response body includes `"reauth_required": true`.
- The frontend detects this and navigates to `/login`.
- **Implication:** All active sessions for this user are NOT invalidated — only the requesting session. The current `MemoryStore` has no mechanism to enumerate and kill other sessions by username. This is acceptable for a single-user self-hosted dashboard (which VexBoard is, by design — one admin account).
- For future multi-session hardening: replace `MemoryStore` with a SQLite-backed session store that can delete by username. This is out of scope for this feature.

### 6.3 Input Validation
- Backend validates: `current_password` non-empty (implicit — bcrypt will fail), `new_username` non-empty and trimmed, `new_password` minimum 8 characters.
- Username uniqueness is enforced by the SQLite `UNIQUE` constraint on `users.username`; a conflict returns HTTP 409.
- Frontend performs mirror validation before sending the request to provide immediate feedback.

### 6.4 Duplicate Username Collision
- The `UPDATE users SET username = ? WHERE id = ?` query will fail with a SQLite `UNIQUE` constraint violation if the new username is already taken.
- The handler catches the DB error and returns HTTP 409 Conflict with `{"error": "Username already taken"}`.

### 6.5 PAM Mode Protection
- The `update_me` handler in PAM mode is compiled as a stub that returns HTTP 405 Method Not Allowed.
- The frontend hides credential change UI when `auth_mode == "pam"`, providing defense in depth.

### 6.6 CSRF Considerations
- The existing application uses `tower-sessions` without explicit CSRF protection (consistent with current posture). The `SameSite` cookie attribute and the `Authorization` header in session cookies provide baseline protection for a self-hosted single-origin deployment.
- No regression introduced by this feature.

### 6.7 No Hardcoded Secrets
- No secrets are introduced. The session secret remains in `AppConfig.auth.secret`, sourced from `VEXBOARD_AUTH_SECRET` environment variable.

---

## 7. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| User locked out after username change (session invalidated, forgets new name) | Low | Medium | The re-auth flow navigates to `/login`; login page has no username hint. Acceptable UX for self-hosted. |
| Race condition: two parallel PATCH requests | Very Low | Low | SQLite serializes writes; second request will either succeed or fail the uniqueness constraint. No data corruption. |
| bcrypt cost on WASM | N/A | N/A | bcrypt runs server-side only. |
| `MemoryStore` session loss on server restart | Already present | Medium | Pre-existing issue; session store replacement is a separate concern. |
| Leptos 0.8 `LocalResource` cache invalidation after credential change | Low | Low | After a credential change `session.flush()` triggers unauthenticated state; the frontend redirects to `/login`, so stale `UserMenu` resource state is never displayed. |
| Dropdown z-index conflict with service card modals (z-index 50) | Low | Low | `.user-menu-dropdown` uses `z-index: 100`, above the existing modal overlay at z-index 50. |
| Username display too long for MetricBar | Low | Low | CSS `max-width: 120px; overflow: hidden; text-overflow: ellipsis` applied to `.user-menu-username`. |

---

## Appendix A: File Change Summary

| File | Change Type | Description |
|------|------------|-------------|
| `crates/vexboard-server/src/api/auth.rs` | Modify | Add `UpdateMeRequest`, `AUTH_MODE` const, extend `me()`, add `update_me()`, update router |
| `crates/vexboard-frontend/src/components/user_menu.rs` | **New** | Full `UserMenu` component with dropdown, settings modal, credential form |
| `crates/vexboard-frontend/src/components/mod.rs` | Modify | Add `pub mod user_menu;` |
| `crates/vexboard-frontend/src/components/metric_bar.rs` | Modify | Add `UserMenu` to right side of metric bar |
| `crates/vexboard-frontend/style/main.css` | Modify | Add user menu CSS rules to `@layer components` |

**Total files touched: 5** (1 new, 4 modified). No new Cargo dependencies. No migration required.

---

## Appendix B: API Contract Summary

### `GET /api/v1/auth/me` (modified)

**Auth:** Session cookie required  
**Response 200:**
```json
{
  "user": {
    "username": "admin",
    "auth_mode": "local"
  }
}
```
(or `"auth_mode": "pam"` for Nix builds)

**Response 401:** Not authenticated

---

### `PATCH /api/v1/auth/me` (new)

**Auth:** Session cookie required  
**Available:** Local mode only (PAM returns 405)

**Request body:**
```json
{
  "current_password": "hunter2",
  "new_username": "newname",
  "new_password": "newpassword123"
}
```
Both `new_username` and `new_password` are optional. At least one must produce a change.

**Response 200:**
```json
{ "status": "ok", "reauth_required": true }
```

**Error responses:**

| Status | Meaning |
|--------|---------|
| 401 | Not authenticated (no session) |
| 400 | Validation error (empty username, password < 8 chars) |
| 403 | Wrong current password |
| 405 | Method not allowed (PAM mode) |
| 409 | Username already taken |
| 500 | Database/hash error |
