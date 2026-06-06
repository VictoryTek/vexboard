# Feature Spec: Multi-User Access Control (Roles)
**Phase:** 1 — Research & Specification
**Date:** 2026-06-06
**Scope:** `crates/vexboard-server` (backend) + `crates/vexboard-frontend` (frontend WASM)

---

## 1. Current State Analysis

### Database
- `users` table: `id, username, password_hash, created_at` — no role column.
- All users inserted via `/setup` (first admin) or... there is no user creation endpoint at all. The setup endpoint creates the first admin account; no subsequent users can be created via API.

### Backend
- `require_auth` middleware (`api/mod.rs:23`) reads `username` from session; no role check.
- All protected routes (`services`, `groups`, `quick-links`, `metrics`, `discovery`, `audit`) share a single `protected` router with only `require_auth`.
- `User` model has no `role` field. `UserInfo` (returned by `/me` and login) has no `role` field.
- Session stores only `username`.
- No user management endpoints exist.

### Frontend
- No role/auth context is provided via Leptos context system; `App` only provides `SidebarMode`.
- `MainLayout` does not fetch `/me`; individual pages fetch what they need.
- `ServiceCard` and `QuickLinkCard` always render Edit/Remove buttons.
- Settings page has no user management section.
- Feature #10 (dark/light mode toggle) is already implemented in settings — the `Toggle Theme` button is present.

---

## 2. Problem Definition

All authenticated users have identical full-admin permissions. There is no way to:
- Create additional user accounts (no user management API exists).
- Assign a read-only `viewer` role to a user.
- Prevent a viewer from mutating services, groups, or quick-links.

---

## 3. Proposed Solution Architecture

### 3.1 Roles

Two roles: `admin` and `viewer`.

| Capability | viewer | admin |
|---|---|---|
| View services, groups, quick-links, metrics, audit | ✅ | ✅ |
| Create/edit/delete services, groups, quick-links | ❌ 403 | ✅ |
| Trigger service discovery / claim units | ❌ 403 | ✅ |
| Manage users (create/delete/change role) | ❌ 403 | ✅ |
| Change own password/username | ✅ | ✅ |

### 3.2 Database Migration

New file `crates/vexboard-server/src/db/migrations/003_user_roles.sql`:
```sql
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'admin';
```
SQLite supports `ALTER TABLE ADD COLUMN` with `NOT NULL DEFAULT value`. All existing rows (always `admin` tier since they were created via setup) receive the default safely.

`db/mod.rs` must apply this migration. The existing backfill pattern (check `pragma_table_info`, apply if missing) is used so the migration is idempotent on existing databases.

### 3.3 Backend Model Changes (`db/models.rs`)

- `User`: add `pub role: String`
- `UserInfo`: add `pub role: String`
- New `UserPublic`: `{ id: i64, username: String, role: String, created_at: Option<NaiveDateTime> }` — used in user list response
- New `CreateUserRequest`: `{ username: String, password: String, role: String }` — `role` validated to be `"admin"` or `"viewer"`
- New `UpdateUserRequest`: `{ role: Option<String>, username: Option<String> }` — admin can change role or username of any user

### 3.4 Middleware Changes (`api/mod.rs`)

**Enhance `require_auth`**: No functional change — continues to check `username` in session. Returns `401` if absent.

**New `require_admin` middleware**: Reads `role` from session. Returns `403 Forbidden` if role is not `"admin"`. Also returns `401` if `username` is absent (covers the case where require_admin is applied to admin-only routes without chaining require_auth).

**Router restructuring** — split the single `protected` router into two:

```rust
// Viewer + admin: read-only endpoints
let viewer_protected = Router::new()
    .nest("/api/v1/services", services::read_router())
    .nest("/api/v1/groups", groups::read_router())
    .nest("/api/v1/quick-links", quick_links::read_router())
    .nest("/api/v1/metrics", metrics::router())
    .nest("/api/v1/audit", audit::router())
    .route_layer(middleware::from_fn(require_auth));

// Admin only: mutating endpoints + user management + discovery
let admin_protected = Router::new()
    .nest("/api/v1/services", services::admin_router())
    .nest("/api/v1/groups", groups::admin_router())
    .nest("/api/v1/quick-links", quick_links::admin_router())
    .nest("/api/v1/discovery", crate::discovery::router())
    .nest("/api/v1/users", users::router())
    .route_layer(middleware::from_fn(require_admin));
```

Both are merged into the top-level router.

### 3.5 Resource Router Splits

Each resource module exposes two routers instead of one.

**`api/services.rs`:**
```rust
pub fn read_router() -> Router<AppState> {
    Router::new().route("/", get(list_services))
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/reorder", patch(reorder_services))
        .route("/", post(create_service))
        .route("/{id}", put(update_service).delete(delete_service))
        .route("/{id}/claim", post(claim_service))
}
```

**`api/groups.rs`:**
```rust
pub fn read_router() -> Router<AppState> {
    Router::new().route("/", get(list_groups))
}
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_group))
        .route("/{id}", put(update_group).delete(delete_group))
}
```

**`api/quick_links.rs`:**
```rust
pub fn read_router() -> Router<AppState> {
    Router::new().route("/", get(list_quick_links))
}
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_quick_link))
        .route("/{id}", put(update_quick_link).delete(delete_quick_link))
}
```

The existing `pub fn router()` function in each module can be replaced by `read_router()` + `admin_router()`, or kept and deprecated. To minimize diff, remove `router()` and replace its call sites in `api/mod.rs`.

### 3.6 Auth Handler Changes (`api/auth.rs`)

**Login (non-PAM path):**
- After verifying credentials, also store role in session: `session.insert("role", user.role.clone()).await`.

**Login (PAM path):**
- PAM users are always treated as admins (no DB role available): `session.insert("role", "admin".to_string()).await`.

**`/me` endpoint:**
- Return `role` in the response body. For non-PAM: read from DB user record. For PAM: hardcode `"admin"`.

**`/me` PATCH (update_me):**
- No changes to role via this endpoint — admins change other users' roles via `/api/v1/users/{id}`.

### 3.7 New User Management API (`api/users.rs`)

Mounted at `/api/v1/users` under `admin_protected`.

| Method | Path | Description |
|---|---|---|
| `GET` | `/` | List all users (id, username, role, created_at). Admin only. |
| `POST` | `/` | Create a new user. Body: `{username, password, role}`. Role must be `"admin"` or `"viewer"`. |
| `PATCH` | `/{id}` | Update user role or username. Body: `{role?, username?}`. Cannot change own role. |
| `DELETE` | `/{id}` | Delete user. Cannot delete self (returns 409). |

All endpoints write audit log entries. Password hashed with `bcrypt::DEFAULT_COST`.

### 3.8 Setup Handler Change (`api/setup.rs`)

The `INSERT INTO users` query currently does not include `role`. Since `role` has `DEFAULT 'admin'`, this continues to work without change. No modification needed.

### 3.9 OpenAPI Registration (`api/openapi.rs`)

Add to `paths`:
- `crate::api::users::list_users`
- `crate::api::users::create_user`
- `crate::api::users::update_user`
- `crate::api::users::delete_user`

Add to `components.schemas`:
- `crate::db::models::UserPublic`
- `crate::db::models::CreateUserRequest`
- `crate::db::models::UpdateUserRequest`

### 3.10 Frontend

#### `main.rs` — Role context
In `MainLayout`:
- Fetch `GET /api/v1/auth/me` on mount via `LocalResource`.
- If response is 401, redirect to `/login`.
- On success, provide `role: ReadSignal<String>` via Leptos context (defaults to `"viewer"` until resolved — safe: viewer is the more restrictive role).
- `UserInfo { username: String, role: String }` stored in context as `RwSignal`.

#### `components/service_card.rs` — Optional action props
Change `on_edit` and `on_delete` to `Option<Callback<i64>>`. When `None`, hide the corresponding button. This avoids passing a role prop through `ServiceData` and keeps the component self-contained.

#### `components/quick_link_card.rs` — Same change
Same optional prop pattern for `on_edit` and `on_delete`.

#### `pages/dashboard.rs` — Viewer mode
- Read `UserInfo` from context.
- When building `render_card`: pass `on_edit = None` and `on_delete = None` when `role != "admin"`.
- Same for quick link cards.
- Hide the "+ Add" button and "Manage Groups" menu item for viewers.

#### `pages/settings.rs` — User Management card
- Read `UserInfo` from context.
- Show "User Management" card only when `role == "admin"`.
- Fetch `GET /api/v1/users` to list users.
- List: username, role pill, delete button (not shown for own account).
- Create user form: username input, password input, role select (Admin/Viewer), Submit.
- Change role: inline role selector per user (PATCH).

---

## 4. Implementation Steps

1. Create `crates/vexboard-server/src/db/migrations/003_user_roles.sql`
2. Update `db/mod.rs` — apply migration 003 with idempotent column check
3. Update `db/models.rs` — add `role` to `User`/`UserInfo`, add `UserPublic`, `CreateUserRequest`, `UpdateUserRequest`
4. Update `api/auth.rs` — store role in session at login (both paths), return role from `/me`
5. Update `api/services.rs` — replace `router()` with `read_router()` + `admin_router()`
6. Update `api/groups.rs` — same
7. Update `api/quick_links.rs` — same
8. Create `api/users.rs` — user CRUD handlers
9. Update `api/mod.rs` — add `require_admin`, restructure into viewer + admin routers, add `pub mod users`
10. Update `api/openapi.rs` — register new schemas + paths
11. Update `crates/vexboard-frontend/src/main.rs` — fetch `/me` in `MainLayout`, provide role context
12. Update `crates/vexboard-frontend/src/components/service_card.rs` — optional action props
13. Update `crates/vexboard-frontend/src/components/quick_link_card.rs` — optional action props
14. Update `crates/vexboard-frontend/src/pages/dashboard.rs` — pass None props for viewers, hide Add button
15. Update `crates/vexboard-frontend/src/pages/settings.rs` — User Management card

---

## 5. Dependencies

No new Cargo dependencies. All required crates (`bcrypt`, `sqlx`, `serde`, `axum`, `tower_sessions`, `leptos`, `gloo-net`) are already present.

---

## 6. Configuration Changes

None. Roles are stored in the database, not in configuration.

---

## 7. Build/Test Commands for Phase 3

Same as previous features:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `scripts/preflight.sh`

---

## 8. Files to be Modified / Created

| File | Change |
|---|---|
| `crates/vexboard-server/src/db/migrations/003_user_roles.sql` | New — add role column |
| `crates/vexboard-server/src/db/mod.rs` | Apply migration 003 |
| `crates/vexboard-server/src/db/models.rs` | Add role fields + new DTOs |
| `crates/vexboard-server/src/api/auth.rs` | Store role in session, return role from /me |
| `crates/vexboard-server/src/api/services.rs` | Split router |
| `crates/vexboard-server/src/api/groups.rs` | Split router |
| `crates/vexboard-server/src/api/quick_links.rs` | Split router |
| `crates/vexboard-server/src/api/users.rs` | New — user management CRUD |
| `crates/vexboard-server/src/api/mod.rs` | Add require_admin, restructure routers |
| `crates/vexboard-server/src/api/openapi.rs` | Register new types + paths |
| `crates/vexboard-frontend/src/main.rs` | Role context in MainLayout |
| `crates/vexboard-frontend/src/components/service_card.rs` | Optional action props |
| `crates/vexboard-frontend/src/components/quick_link_card.rs` | Optional action props |
| `crates/vexboard-frontend/src/pages/dashboard.rs` | Viewer-mode UI |
| `crates/vexboard-frontend/src/pages/settings.rs` | User Management card |

---

## 9. Risks and Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Existing sessions after migration have no `role` key | Low | `require_admin` treats missing role as non-admin → returns 403, forces re-login; `require_auth` only checks `username` so viewers still work |
| Admin demotes all admins (no admin left) | Medium | `DELETE /{id}` and `PATCH /{id}/role` check: if changing the last admin, return 409 |
| Admin changes own role to viewer | Medium | `PATCH /{id}` checks session username against target user id; blocks self-demotion |
| SQLite column migration on existing DB | Low | Idempotent check via `pragma_table_info` before ALTER TABLE |
| PAM mode: role is always admin (hardcoded) | Info | PAM is a Linux-only feature for system users; documented in code comment |
| Frontend shows viewer the Add button briefly while /me is loading | Low | Default role in context is `"viewer"` (most restrictive) — Add button is hidden until /me confirms admin |
