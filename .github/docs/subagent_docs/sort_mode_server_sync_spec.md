# Sort Mode Server Sync — Spec

## Current State Analysis

- Sort mode (`AZ` / `Source` / `Group`) is persisted **client-side only**, in browser `localStorage` under key `"vexboard_sort_mode"` — added in a prior fix (`sort_mode_persistence_spec.md`, commit `b8e861f`).
  - Loader/saver: `crates/vexboard-frontend/src/pages/dashboard/mod.rs:36-68` (`load_sort_mode_from_storage`, `save_sort_mode_to_storage`, cfg-gated wasm32/native).
  - Signal init: `crates/vexboard-frontend/src/pages/dashboard/mod.rs:139` — `let (sort_mode, set_sort_mode) = signal(load_sort_mode_from_storage());`
  - Write site: `crates/vexboard-frontend/src/pages/dashboard/mod.rs:223-226`, inside the sort-toggle button `on:click`.
  - Consumed reactively by `ServiceGrid` (:364-373), `QuickLinksSection` (:376-382), `GroupSection` (:385-398).
- Root cause of the user-reported bug: `localStorage` is scoped per browser origin. The user accesses VexBoard from multiple different browsers/devices over the same Tailscale IP. Each browser has independent, empty storage, so the preference set on one device never appears on another — this was misdiagnosed as "not surviving an update/reboot" but is actually "never shared across browsers" by design of the current implementation. Confirmed with user; desired fix is **per-user, server-side** persistence (chosen over a global instance-wide default).
- Auth/session model (`crates/vexboard-server/src/api/auth.rs`): cookie-based `tower_sessions::Session` storing `"username"` and `"role"` strings. No `FromRequestParts` "current user" extractor — every handler pulls `session.get::<String>("username")` directly. This idiom must be followed for the new endpoint.
- Two auth backends exist: local (`users` table, bcrypt) and PAM (`#[cfg(all(unix, feature = "pam-auth"))]`, no local `users` row). **PAM users have no row in `users`**, so a new preference cannot be a column on `users` or FK to `users.id` — it must be keyed by `username` (stable across both auth modes).
- Existing generic KV store: `settings` table (`crates/vexboard-server/src/db/migrations/001_init.sql:45-48`, `key TEXT PRIMARY KEY, value TEXT NOT NULL`), with helpers already implemented in `crates/vexboard-server/src/db/mod.rs:214-233`: `get_setting(pool, key)` / `set_setting(pool, key, value)` (upsert via `ON CONFLICT`). Currently used only for the single global `"auth_mode"` key (`crates/vexboard-server/src/api/settings.rs`, admin-only route).
- `GET /api/v1/auth/me` (`crates/vexboard-server/src/api/auth.rs:299-338`) already returns a hand-built JSON blob (`username`, `role`, `auth_mode`) sourced from the session — the natural place to add the stored preference to the read side.
- `crates/vexboard-server/src/api/mod.rs:58-71`: `auth::router()` is nested directly under the **public** routes (no `require_auth`/`require_admin` middleware layer) — each handler does its own session check. A new mutating route for "the current user's own preference" belongs here (any authenticated user may set their own value; no admin gate needed), not under `admin_protected`.

## Problem Definition

The dashboard sort-mode preference must be shared across all browsers/devices for the same logged-in user, instead of being trapped in a single browser's `localStorage`.

## Proposed Solution

Reuse the existing global `settings` key/value table with a **per-user-namespaced key**, rather than introducing a new table/migration:

- Key format: `format!("dashboard_sort_mode:{username}")`, value one of `"az" | "source" | "group"` (same encoding already used for `localStorage`).
- This requires **zero schema migration** — `get_setting`/`set_setting` already exist and already upsert correctly. Chosen over a dedicated `user_settings` table because it needs no new migration, no idempotent-guard boilerplate in `run_migrations`, and the `settings` table's only existing consumer (`auth_mode`) already establishes the "string key, string value" convention. This keeps the change minimal (Simplicity First) while remaining correct for both local and PAM auth (keyed by `username`, not `user_id`).
- No new external dependency — no Context7 verification required (internal change only, existing `sqlx`, `axum`, `tower_sessions`, `gloo_net` all already in use).

### Backend changes (`crates/vexboard-server/src/api/auth.rs`)

1. Add a request DTO:
   ```rust
   #[derive(Debug, Deserialize, utoipa::ToSchema)]
   pub(crate) struct UpdateSortModeRequest {
       sort_mode: String,
   }
   ```
2. Add a new handler `update_sort_mode`, **not** cfg-gated by `pam-auth` (must work for both auth modes, unlike the credential-change `update_me`):
   ```rust
   async fn update_sort_mode(
       State(state): State<AppState>,
       session: Session,
       Json(payload): Json<UpdateSortModeRequest>,
   ) -> impl IntoResponse {
       let username = match session.get::<String>("username").await {
           Ok(Some(u)) => u,
           _ => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Not authenticated"}))),
       };

       if !matches!(payload.sort_mode.as_str(), "az" | "source" | "group") {
           return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid sort_mode"})));
       }

       let key = format!("dashboard_sort_mode:{username}");
       if db::set_setting(&state.db, &key, &payload.sort_mode).await.is_err() {
           return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"})));
       }

       (StatusCode::OK, Json(json!({"ok": true})))
   }
   ```
3. Register in `router()`: add `.route("/me/sort-mode", put(update_sort_mode))` (import `put` alongside existing `get`/`post`).
4. Modify `me` (:310) to also read and return the preference — add `State(state): State<AppState>` to its extractors, fetch via `db::get_setting(&state.db, &format!("dashboard_sort_mode:{username}"))` defaulting to `"az"` on `None`/error, and include `"dashboard_sort_mode": sort_mode` in the response JSON.
5. Add a matching `#[utoipa::path(...)]` doc block for the new route, consistent with the existing ones on `login`/`logout`/`me`.

### Frontend changes (`crates/vexboard-frontend/src/pages/dashboard/mod.rs`)

1. Remove `load_sort_mode_from_storage` / `save_sort_mode_to_storage` (lines 36-68) — no longer needed, replaced by server calls. This also removes the now-unused `#[allow(dead_code)]`/cfg-gating for this pair.
2. Add two async functions near `fetch_services` (:403+):
   ```rust
   #[derive(serde::Deserialize)]
   struct MeUserSortMode { dashboard_sort_mode: Option<String> }
   #[derive(serde::Deserialize)]
   struct MeSortModeWrapper { user: MeUserSortMode }

   pub(super) async fn fetch_sort_mode() -> SortMode {
       match gloo_net::http::Request::get("/api/v1/auth/me").send().await {
           Ok(r) if r.ok() => r
               .json::<MeSortModeWrapper>()
               .await
               .ok()
               .and_then(|w| w.user.dashboard_sort_mode)
               .map(|s| match s.as_str() {
                   "source" => SortMode::Source,
                   "group" => SortMode::Group,
                   _ => SortMode::AZ,
               })
               .unwrap_or(SortMode::AZ),
           _ => SortMode::AZ,
       }
   }

   pub(super) async fn save_sort_mode(mode: SortMode) {
       let val = match mode {
           SortMode::AZ => "az",
           SortMode::Source => "source",
           SortMode::Group => "group",
       };
       let _ = gloo_net::http::Request::put("/api/v1/auth/me/sort-mode")
           .json(&serde_json::json!({ "sort_mode": val }))
           .expect("serializable body")
           .send()
           .await;
   }
   ```
   (A duplicate `GET /api/v1/auth/me` call versus the one `main.rs` already makes for `CurrentUser` is accepted — matches the existing decoupled-per-component fetch style used throughout the frontend; no shared API client exists to dedupe through.)
3. Keep the signal initialized to `SortMode::AZ` at :139 (can't synchronously await), then load the real value asynchronously:
   ```rust
   let sort_mode_loaded = LocalResource::new(|| async move { fetch_sort_mode().await });
   Effect::new(move |_| {
       if let Some(mode) = sort_mode_loaded.get() {
           set_sort_mode.set(mode);
       }
   });
   ```
   Placed near the other `LocalResource::new` calls (:130-133).
4. Update the sort-toggle `on:click` (:223-226):
   ```rust
   on:click=move |_| {
       set_sort_mode.set(mode);
       spawn_local(async move {
           save_sort_mode(mode).await;
       });
   }
   ```

## Implementation Steps

1. Backend: `crates/vexboard-server/src/api/auth.rs` — add `UpdateSortModeRequest`, `update_sort_mode` handler, route registration, `me` handler changes (as above).
2. Frontend: `crates/vexboard-frontend/src/pages/dashboard/mod.rs` — remove localStorage helpers, add `fetch_sort_mode`/`save_sort_mode`, wire `LocalResource` + `Effect`, update click handler.
3. No migration files, no `Cargo.toml` changes, no `config/default.toml` changes.

## Dependencies

None new. All of `sqlx`, `tower_sessions`, `axum`, `gloo_net`, `serde`/`serde_json` are already workspace dependencies exercised in the exact same way elsewhere in these two files.

## Configuration Changes

None.

## Risks and Mitigations

- **Brief AZ flash on load:** unlike the old synchronous `localStorage` read, the server round-trip means the grid renders in `AZ` order for one frame before `fetch_sort_mode` resolves. **Mitigation:** accepted trade-off (typical LAN/Tailscale round-trip is tens of ms); no loading-skeleton added, per Simplicity First — this matches how `services`/`quick_links`/`groups` already pop in via `LocalResource` on this same page.
- **PAM users and the `settings` key namespace:** using `username` (not `user_id`) as the key component means a username rename (`update_me`, local-auth only) orphans the old preference row. **Mitigation:** acceptable minor edge case (falls back to default `"az"`, not an error); not worth extra migration/lookup logic for a cosmetic preference. Not applicable to PAM users (no username-change endpoint exists in PAM mode — `update_me` returns 405 under `pam-auth`).
- **Unauthenticated `PUT`:** handler explicitly checks `session.get::<String>("username")` and returns `401` before touching the DB, matching `update_me`'s existing pattern — no new exposure.
- **Invalid `sort_mode` value:** validated against the exact 3 allowed strings server-side before storage; malformed values are rejected with `400` rather than silently stored and later mis-mapped to `AZ` on read.
