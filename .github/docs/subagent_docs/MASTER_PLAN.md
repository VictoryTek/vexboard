# VexBoard — Master Plan

Consolidated and deduplicated from:
- `ANALYSIS_BUGS.md` (logic errors, security, data corruption)
- `ANALYSIS_ARCH.md` (structural debt, design problems)
- `ANALYSIS_FEATURES.md` (missing or partially-implemented features)

Source references use the shorthand: **B**=BUGS, **A**=ARCH, **F**=FEATURES.

Legend: `[BUG]` = incorrect/broken behavior · `[SEC]` = security impact · `[ARCH]` = structural debt · `[FEAT]` = feature work

---

## HIGH PRIORITY

### Security

- [x] **SEC-1 — Sessions never expire; role/deletion changes don't invalidate live sessions** *(B-H2, B-H4, A-A1, A-A2)*
  `auth.secret` and `session_ttl_hours` are deserialized and never read. `SessionManagerLayer` is built without `.with_expiry(...)`, so the configured 7-day TTL has no effect. Demoting or deleting a user has no effect on their active sessions — they retain full (possibly admin) access until they voluntarily log out. The NixOS module enforces a `secretFile` workflow for a secret the server never uses (security theater).
  **Fix:** Wire `session_ttl_hours` into `SessionManagerLayer::with_expiry(Expiry::OnInactivity(...))`. On role change, rename, or delete — delete the target user's rows from `tower_sessions`. Fix the Nix preStart guard to only gate on options that actually do something.
  *Files:* `src/config.rs:40-41`, `src/main.rs:127-130`, `src/session_store.rs`, `src/middleware/auth.rs:20-38`, `src/api/users.rs:182-305,325-413`, `nix/module.nix:68-91,132-150`

- [x] **SEC-2 — X-Forwarded-For trusted unconditionally — rate limit trivially bypassable** *(B-H1, A-A7)*
  `client_ip()` prefers the first client-supplied XFF header entry over the real socket address. Any client can send a fresh random IP per request to bypass the rate limiter entirely. The `LoginRateLimiter` HashMap is never evicted, so spoofing unique IPs grows memory unboundedly. Spoofed IPs are also written to the audit log.
  **Fix:** Add `auth.behind_proxy = false` config flag. Only honor XFF (last hop, not first) when enabled. Evict empty `VecDeque`s from the rate limiter map.
  *Files:* `src/api/auth.rs:24-35`, `src/rate_limit.rs`

### Data Loss / Functional Breakage

- [x] **BUG-1 — Edit Service modal silently resets probe settings on every save** *(B-H3, A-H1)*
  The modal receives the service's real `probe_enabled`/`probe_interval` but the Save handler hardcodes `probe_enabled: true, probe_interval: 30`. Editing anything (rename, icon, URL) re-enables probing on a disabled service and resets custom intervals. No probing UI exists in the modal so users cannot see this happening.
  **Fix:** Pass `initial.probe_enabled` and `initial.probe_interval` through to the save payload instead of hardcoding.
  *Files:* `src/components/modal_edit.rs:188-197`

- [x] **BUG-2 — Role `<select>` uses wrong DOM cast — new users always created as viewer** *(B-H6)*
  The `on:change` handler casts the `<select>` element as `HtmlInputElement`; `dyn_into` always fails silently so `new_role` stays `"viewer"` regardless of what the admin selects. The "Admin" option is inert.
  **Fix:** Cast to `HtmlSelectElement` and enable the `HtmlSelectElement` web-sys feature. Read `.value()` from it.
  *Files:* `src/pages/settings.rs:330-342`, `crates/vexboard-frontend/Cargo.toml`

- [x] **BUG-3 — Claimed Docker/Podman containers probed as systemd units — always report "down"** *(B-H5)*
  When the discovery panel claims a Docker container, it posts `systemd_unit: <container-name>`. The probe scheduler gives `systemd_unit` priority over `url`, so the container is probed via `unit_active_state()`, which looks for a nonexistent systemd unit and returns `"inactive"` forever. The dashboard shows a permanent red dot for every claimed container.
  **Fix:** Only set/use `systemd_unit` when `discovery_source == "systemd"`. When source is `"docker"` or `"podman"`, probe via URL only.
  *Files:* `src/components/discovery_panel.rs:98-99`, `src/probe/mod.rs:38-44`, `src/probe/uptime.rs:195-206`

### Features (High Value, Low Effort — Infrastructure Already Exists)

- [x] **FEAT-1 — Live service-status SSE stream** *(F-F1, A-H4)*
  The backend has a complete pub/sub pipeline: `probe_tx: broadcast::Sender<ProbeEvent>` in `AppState`, every probe broadcasts a serializable `ProbeEvent`, and the SSE machinery is proven in `api/metrics.rs`. No API handler ever subscribes to `probe_tx` — only the webhook notifier does. The dashboard compensates with a hard-coded sleep-then-refetch hack.
  **Fix:** Add `GET /api/v1/services/stream` (viewer-protected) forwarding `ProbeEvent`s as SSE. In `service_grid.rs`, subscribe once and patch card status/latency signals directly.
  *Files:* `src/api/metrics.rs`, `src/main.rs:39,117`, `src/probe/uptime.rs:35-42`, `src/components/service_grid.rs`

- [x] **FEAT-2 — Dismiss discovered services** *(F-F2)*
  The Settings page tells users they can "claim or dismiss" discovered services. Dismiss does not exist anywhere — no handler, no button, no persistence. Every unclaimed unit reappears forever.
  **Fix:** Add `dismissed_units` table (`source`, `unit_name`, `created_at`). Add `POST /api/v1/discovery/dismiss` and `DELETE` endpoint (admin). Filter dismissed names in discovery loops. Add "Dismiss" button in `discovery_panel.rs`.
  *Files:* `src/discovery/systemd.rs`, `src/discovery/docker.rs`, `src/components/discovery_panel.rs`

- [x] **FEAT-3 — Uptime history endpoint + sparkline on service cards** *(F-F5)*
  `probe_results` already stores up to 100 results per service with status, latency, and timestamp. The only read of that table is "latest row per service." All that history is collected, trimmed, and never shown.
  **Fix:** Add `GET /api/v1/services/{id}/history?limit=100` (viewer-protected). Add latency sparkline + uptime-% strip to `service_card.rs`.
  *Files:* `src/api/services.rs`, `src/components/service_card.rs`

---

## MEDIUM PRIORITY

### Security

- [x] **SEC-3 — No session ID rotation on login (session fixation)** *(B-M1)*
  Neither login path calls `session.cycle_id()` before inserting credentials. Call it on successful login.
  *Files:* `src/api/auth.rs:84-130,132-211`

- [x] **SEC-4 — Login rate limiter panics during first minute after boot** *(B-M2)*
  `Instant - Duration` panics on underflow. On boot-started deployments, any login within `window` seconds of boot panics the handler. Use `now.checked_sub(self.window)`.
  *Files:* `src/rate_limit.rs:27`

- [x] **SEC-5 — `/auth/me` defaults missing role to `"admin"`** *(B-M3)*
  If `role` is absent from a session (possible for sessions created before roles were added), `me()` returns `role: "admin"`. Change fallback to `"viewer"`.
  *Files:* `src/api/auth.rs:260-268`

- [x] **SEC-6 — Last-admin guard fails open on DB error** *(B-M4)*
  If the `COUNT(*)` query errors, `unwrap_or(2)` assumes 2 admins and allows demoting/deleting the last admin. Return 500 on count failure instead.
  *Files:* `src/api/users.rs:234-238,369-373`

- [x] **SEC-7 — Audit log exposed to viewer role** *(A-A8)*
  `/api/v1/audit` is under `viewer_protected`. Audit entries contain login-failure usernames and client IPs — viewers can reconstruct the user list and watch admin activity.
  *Files:* `src/api/mod.rs:22-28`

- [x] **SEC-8 — PAM mode grants every OS account admin** *(B-M14, A-A9)*
  With `pam-auth`, any PAM-authenticating user gets `role = "admin"`. `pam_acct_mgmt` is never called (expired/locked accounts can authenticate). PAM call is synchronous on the async runtime (blocks tokio worker ~2s on failure).
  **Fix:** Add `auth.pam_admin_users` allowlist. Call `pam_acct_mgmt`. Wrap in `spawn_blocking`.
  *Files:* `src/pam_auth.rs:73-100`, `src/api/auth.rs:84-98,257-258`

### Bugs

- [x] **BUG-4 — `update_service` can never clear `group_id` — "No group" is a no-op** *(B-M5, A-I3)*
  `payload.group_id.or(existing.group_id)` means JSON `null` keeps the old group. Same pattern blocks clearing `discovery_source` and a group's `icon`/`color`. Use a sentinel (e.g. `0` or double-`Option`) to represent "clear to NULL."
  *Files:* `src/api/services.rs:272,286`, `src/api/groups.rs:164-167`, `src/pages/dashboard/modals.rs:92-100`

- [x] **BUG-5 — Per-service `probe_interval` ignored — all services probe at global interval** *(B-M6, A-A3, F-F3)*
  The scheduler probes every enabled service each tick at `default_interval_secs`. `probe_interval` flows through schema, model, DTOs, OpenAPI, and frontend — but is never read. Keep a `HashMap<i64, Instant>` of last-probed times; tick at a short base interval and only probe when due.
  *Files:* `src/probe/mod.rs:19-48`

- [x] **BUG-6 — New `reqwest::Client` built per probe; timeout removed on fallback** *(B-M7, A-A4)*
  A fresh client (new connection pool, TLS config) is built per service per cycle. `unwrap_or_default()` on build failure substitutes a client with **no timeout**. Build one shared client at startup (the notifier already demonstrates this pattern).
  *Files:* `src/probe/uptime.rs:58-62`, `src/main.rs:118-121`

- [x] **BUG-7 — Expired sessions never deleted — unbounded table growth** *(B-M8)*
  `load()` filters expired rows but never deletes them. No periodic cleanup. Use tower-sessions' `continuously_delete_expired` pattern.
  *Files:* `src/session_store.rs:61-99`

- [ ] **BUG-8 — Discovery panel posts to `/services` directly — duplicate claims possible** *(B-M9, A-A6, A-H5)*
  The `claim_service` endpoint has a uniqueness check, but the discovery panel bypasses it by posting to `POST /api/v1/services`. Double-clicking "Add" creates duplicate rows. Add a `UNIQUE` constraint on `services.systemd_unit` and route the panel through the claim endpoint (or add the check to the create path).
  *Files:* `src/api/services.rs:501-547`, `src/components/discovery_panel.rs:109-111`, `src/db/migrations/001_init.sql:14`

- [ ] **BUG-9 — A-Z drag-and-drop reorders wrong items on sort_order ties** *(B-M10)*
  The rendered list sorts by `sort_order` then name, but the drop handler applies indices to the raw fetch order (`ORDER BY sort_order ASC` only — ties are arbitrary). Apply the same `sort_by` tiebreak before computing indices in the A-Z handler.
  *Files:* `src/pages/dashboard/service_grid.rs:414-417,451-462`

- [ ] **BUG-10 — Group reordering no-op — all new groups get `sort_order = 0`** *(B-M11)*
  `do_create` always sends `sort_order: 0`. Swapping two zeros in `do_move` changes nothing. Assign `max(sort_order)+1` on create.
  *Files:* `src/components/modal_groups.rs:95,138-167`

- [ ] **BUG-11 — Disk I/O metrics zero on VMs and many real systems** *(B-M12)*
  `read_disk()` only matches `sd?` (3 chars) or `nvme*n1`. Excludes `vda`/`vdb` (KVM), `xvda` (Xen), `mmcblk0` (RPi), `md0`/`dm-0` (RAID/LVM), `nvme0n2+`, `sdaa+`. Silently reports 0 on those systems.
  *Files:* `src/metrics/system.rs:192-226`

- [ ] **BUG-12 — Frontend swallows all mutation errors — failures are invisible** *(B-M13)*
  Nearly every write is `let _ = req.send().await` followed by unconditional `refetch()`. A 409, 403, or 500 produces no user feedback. Match the pattern in the Add User form (reads and displays the error body).
  *Files:* `src/pages/dashboard/modals.rs`, `src/pages/dashboard/service_grid.rs`, `src/pages/settings.rs`, `src/components/modal_groups.rs`, `src/components/discovery_panel.rs`

- [ ] **BUG-13 — Docker TCP endpoints silently ignored** *(B-M15)*
  `socket_host()` parses `tcp://` URLs but the loop skips them (`Path::new(socket).exists()` is false for URLs). `Docker::connect_with_socket` is Unix-only. Either support TCP via `connect_with_http` or reject non-path entries loudly at startup.
  *Files:* `src/discovery/docker.rs:53-56,78-88,98`

- [ ] **BUG-14 — systemd discovery does O(units) DB query per unit per pass; logs at `info!` per unit** *(B-M16)*
  Per candidate unit every 60s: one `SELECT EXISTS` round-trip, then D-Bus reads, `/proc` scans, possibly `podman port` subprocesses. Fetch all claimed units once per pass. Cache URL hints between passes. Change per-unit log to `debug!`.
  *Files:* `src/discovery/systemd.rs:124-139`

### Architecture

- [ ] **ARCH-1 — `tags` feature is a stub — schema/DTO/API exists, no frontend surface** *(A-H2)*
  Column, DTO fields, JSON handling, and OpenAPI schema all exist. No frontend reads or writes tags. Either finish the tags UI or strip the field end-to-end.
  *Files:* `src/db/migrations/001_init.sql:24`, `src/api/services.rs:120-132,291-303`

- [ ] **ARCH-2 — `visible` flag has no management surface — hides services permanently** *(A-H3)*
  The only list endpoint filters invisible services out; there is no admin listing or UI toggle. A service set `visible=false` disappears from every UI including the edit path.
  *Files:* `src/api/services.rs:54`, `src/db/models.rs:28,66,81`

- [ ] **ARCH-3 — No shared error type — DB failures silently coerced to behavior-changing defaults** *(A-I1)*
  `unwrap_or(None)` on username-uniqueness check reads as "name available" on DB error. `unwrap_or(2)` on admin count weakens the last-admin guard. `thiserror` is already in the dependency tree for exactly this. Create a shared `AppError` type with `IntoResponse`.
  *Files:* `src/api/users.rs:96-100,234-238`, `src/api/services.rs:71-82`, `src/discovery/docker.rs:176-183`

- [ ] **ARCH-4 — HTTP method and response shape vary per resource** *(A-I2)*
  PUT used for partial updates (PATCH semantics) in services/groups/quick_links; PATCH used in users/auth. Response bodies: `{"status":"updated"}` vs `{"ok":true}` vs `{"status":"ok"}` for a creation. Standardize on PATCH for partial updates; return 201+`{"id":…}` for all creates.
  *Files:* `src/api/services.rs:34`, `src/api/groups.rs:22`, `src/api/quick_links.rs:22`, `src/api/users.rs:20`, `src/api/auth.rs:21,453`, `src/api/setup.rs:133`

- [ ] **ARCH-5 — No frontend API client layer — DTOs and fetches duplicated across files** *(A-I4)*
  Four distinct "group" types across four files; `/api/v1/groups` fetched independently in three places; `MeResponse` refetches `/auth/me` even though `MainLayout` already fetched it into context; `is_admin` closure copy-pasted in four files. Create a shared `api.rs` module with shared response structs.
  *Files:* `src/pages/dashboard/mod.rs`, `src/components/modal_edit.rs`, `src/components/discovery_panel.rs`, `src/components/modal_groups.rs`, `src/pages/settings.rs`, `src/components/user_menu.rs`

- [ ] **ARCH-6 — Large-scale copy-paste in frontend components** *(A-I5)*
  The ~70-line draggable card wrapper appears three times nearly verbatim in `service_grid.rs`. `extract_favicon_url` duplicated character-for-character in two files. Icon/button blocks duplicated between `service_card.rs` and `quick_link_card.rs`. User-list refresh fetch inlined three times in `settings.rs`.
  *Files:* `src/pages/dashboard/service_grid.rs`, `src/components/modal_edit.rs`, `src/components/quick_link_modal.rs`, `src/components/service_card.rs`, `src/components/quick_link_card.rs`

- [ ] **ARCH-7 — `thiserror` declared and never used** *(A-D1)*
  No `#[derive(Error)]` or `thiserror::` reference in the server crate. Remove or use it (pairs with ARCH-3).
  *Files:* `Cargo.toml:23`, `crates/vexboard-server/Cargo.toml:32`

### Features

- [ ] **FEAT-4 — Tags UI: input in edit modal, chips on cards, filter bar** *(F-F6)*
  Backend is 100% done. Pure frontend work: tags input in `modal_edit.rs`, tag chips on `ServiceCard`, client-side filter bar above the service grid.

- [ ] **FEAT-5 — Audit log viewer page** *(F-F7, A-H7)*
  Full audit pipeline exists on the backend (table, indexes, paginated API). The frontend never calls it. Add an Audit Log page with sidebar entry rendering the paginated table.

- [ ] **FEAT-6 — Webhook management via API/UI** *(F-F8)*
  Complete delivery engine exists but is TOML-only (restart required). The `settings` table exists but is unused. Store webhooks in DB; expose admin CRUD; add Settings card; have notification loop reload from DB.

- [ ] **FEAT-7 — Service start/stop/restart actions** *(F-F9)*
  zbus and bollard are both linked in; `StartUnit`/`StopUnit`/`RestartUnit` (systemd) and `restart_container` (Docker) are available. Add `POST /api/v1/services/{id}/action` (admin, audited). Add confirm-gated restart button on service card.

- [ ] **FEAT-8 — Export / import dashboard configuration** *(F-F10)*
  All entities derive `Serialize`/`Deserialize`. No backup/restore story. Add `GET /api/v1/export` and `POST /api/v1/import`. Download/Upload pair on Settings page.

---

## LOW PRIORITY

### Architecture

- [ ] **ARCH-8 — Hand-rolled migration runner while sqlx `migrate` feature is compiled in** *(A-A10)*
  Migrations applied via `include_str!` + column-existence probes. `003`/`004` gated by Rust column checks. The `migrate` feature (already compiled in) would replace this with a versioned `_sqlx_migrations` table.

- [ ] **ARCH-9 — `assets_path = "embedded"` is a misnomer; config and assets are CWD-relative** *(A-A11, B-L14)*
  "embedded" actually means `./assets` relative to process CWD. Binary only starts when launched from a directory containing `config/default.toml`. Rename sentinel or embed dist output (e.g. `rust-embed`).

- [ ] **ARCH-10 — D-Bus proxy declared twice; port-selection algorithm implemented twice** *(A-I7)*
  `src/discovery/systemd.rs` and `src/probe/uptime.rs` each declare an identical `ManagerProxy` + unit-info struct. The port-selection heuristic is implemented over bollard structs and over `docker port` text output separately.

- [ ] **ARCH-11 — CI and preflight contradict each other about the frontend crate** *(A-S6)*
  CI runs `cargo clippy --workspace` and `cargo test --workspace` natively. Preflight and CLAUDE.md forbid workspace-wide native builds. Reconcile: either confirm Leptos CSR compiles natively (update docs/preflight) or fix CI to scope to `-p vexboard-server`.

- [ ] **ARCH-12 — `discovery` module mixes HTTP handlers with background infrastructure** *(A-S2)*
  `list_discovered` and `trigger_refresh` handlers live next to D-Bus/bollard scan loops. Move handlers to `api/discovery.rs` to match the rest of the API surface.

- [ ] **ARCH-13 — Four different router-construction conventions** *(A-S1)*
  `read_router()`+`admin_router()` (services/groups/quick_links) vs single `router()` (users/audit/auth) vs inline in `api/mod.rs` (setup) vs outside `api/` entirely (discovery). Standardize.

- [ ] **ARCH-14 — Frontend component naming mixes conventions; `components/mod.rs` re-exports only one item** *(A-S3, A-S4)*
  `modal_edit.rs`/`modal_groups.rs` (prefix) vs `quick_link_modal.rs` (suffix); `status_badge.rs` exports `StatusDot`. `mod.rs` re-exports only `UserMenu`. Pick one convention for each.

- [ ] **ARCH-15 — Overlapping user DTOs from different eras** *(A-H8)*
  `UserInfo` (used in one place), `UserPublic`, and `/me` builds its JSON by hand. `SetupRequest` lives in `api/setup.rs` while all other DTOs live in `db/models.rs`. Consolidate.

- [ ] **ARCH-16 — `js-sys` unused in frontend** *(A-D2)*
  No `js_sys::` reference in `vexboard-frontend/src`. Remove the direct dependency.

- [ ] **ARCH-17 — `tower` in runtime dependencies, only used by tests** *(A-D3)*
  Move to `[dev-dependencies]`.

- [ ] **ARCH-18 — `bollard` 0.17 using legacy options API** *(A-D4)*
  bollard 0.18+ deprecated `ListContainersOptions::<String>` in favor of a query-builder API.

- [ ] **ARCH-19 — sqlx `migrate` feature compiled in and unused** *(A-D6)*
  Either adopt `sqlx::migrate!` (replaces ARCH-8) or drop the feature flag.

- [ ] **ARCH-20 — Three styling mechanisms used interchangeably** *(A-I6)*
  CSS classes, inline `style="…"` strings, and raw JS-string `onmouseover="this.style.color=…"` event attributes alongside reactive `style=move ||` closures. JS-string hovers duplicate what `:hover` in `main.css` already does. Standardize on CSS classes + reactive closures.

- [ ] **ARCH-21 — Tracing instrumentation inconsistent** *(A-I8)*
  All handlers in `services`/`users`/`groups`/`auth` carry `#[tracing::instrument]`; none in `quick_links.rs` do. Log message style varies.

- [ ] **ARCH-22 — `DiscoveredPage` on_added wired to no-op** *(A-H6)*
  The `on_added` prop exists to let a parent refresh after a claim; the only caller passes an empty closure.

- [ ] **ARCH-23 — `settings` table created and never referenced** *(A-S5, B-L1)*
  Dead schema in every fresh database. Either use it (pairs with FEAT-6) or drop it.

### Bugs

- [ ] **BUG-15 — Audit pagination unstable on timestamp ties** *(B-L3)*
  `ORDER BY created_at DESC` with second-resolution `DATETIME` causes same-second entries to repeat or vanish across pages. Add `, id DESC`.
  *Files:* `src/api/audit.rs:67-68`

- [ ] **BUG-16 — Rate limiter counts successful logins against the budget** *(B-L4)*
  `check()` records an attempt before the credential check; successes are never refunded. Count only failures.
  *Files:* `src/api/auth.rs:70`, `src/rate_limit.rs:25-38`

- [ ] **BUG-17 — `parse_port_from_listen_address` misparses bare IPv6 addresses** *(B-L5)*
  `"::1"` splits on the last `:` and yields port 1.
  *Files:* `src/discovery/systemd.rs:657-672`

- [ ] **BUG-18 — Network metrics double-count traffic on Docker hosts; display lifetime totals as rate** *(B-L6)*
  Sums every non-`lo` interface — bridge/veth traffic counted twice on Docker hosts. Metric bar renders cumulative since-boot counter in a position that reads as a live rate.
  *Files:* `src/metrics/system.rs:167-189`, `src/components/metric_bar.rs:116-139`

- [ ] **BUG-19 — CPU sampling: comment says 1s, code sleeps 250ms; snapshot endpoint blocks caller** *(B-L7)*
  Doc/code mismatch. Every `/api/v1/metrics/snapshot` call blocks ~250ms. Maintain previous sample in state.
  *Files:* `src/metrics/system.rs:96-101`

- [ ] **BUG-20 — Edit modal favicon auto-fill clobbers manually chosen icon on first URL keystroke** *(B-L8)*
  `icon_auto` starts `true` even when editing a service with a hand-set icon. Initialize to `initial.icon.is_empty()`.
  *Files:* `src/components/modal_edit.rs:62,95-105`, `src/components/quick_link_modal.rs:46`

- [ ] **BUG-21 — Add Service modal retains stale form state across opens** *(B-L9)*
  Signals initialized once; reopening after a save shows the previous service's values.
  *Files:* `src/pages/dashboard/modals.rs:70-77`

- [ ] **BUG-22 — Group rename fires twice on Enter** *(B-L10)*
  Enter triggers `do_rename`, closes editor, blurs input, `on:blur` calls `do_rename` again → two PUTs.
  *Files:* `src/components/modal_groups.rs:292-299`

- [ ] **BUG-23 — SSE `EventSource` and listener leaked — not closed on cleanup** *(B-L11)*
  Use Leptos' `on_cleanup` to `es.close()` and drop the listener.
  *Files:* `src/components/metric_bar.rs:71-86`

- [ ] **BUG-24 — `Request::json(...).unwrap()` in WASM handlers panics app on error** *(B-L12)*
  Inconsistent with the rest of the codebase that uses `if let Ok(req)`.
  *Files:* `src/pages/login.rs:24`, `src/pages/setup.rs:31`, `src/components/user_menu.rs:125`

- [ ] **BUG-25 — `server_services_only` rustdoc describes wrong behavior** *(B-L13)*
  Rustdoc says it filters by unit-file location; code filters by `sub_state == "running"`.
  *Files:* `src/config.rs:68-72`

- [ ] **BUG-26 — Username inputs trim-validated but stored raw** *(B-L15)*
  `" bob"` and `"bob"` are distinct users that render identically. Trim before binding.
  *Files:* `src/api/users.rs:77`, `src/api/auth.rs:369-376`, `src/api/setup.rs:95`

- [ ] **BUG-27 — bcrypt silently truncates passwords beyond 72 bytes** *(B-L16)*
  No max-length validation; characters beyond 72 bytes are silently ignored at both set and verify time.
  *Files:* `src/api/setup.rs:107`, `src/api/users.rs:109`, `src/api/auth.rs:417`

- [ ] **BUG-28 — No confirmation on destructive actions** *(B-L17)*
  One click permanently deletes a service, quick link, group, or user — with errors also suppressed (BUG-12). At minimum gate user deletion behind a confirm dialog.
  *Files:* `src/pages/dashboard/service_grid.rs:70-77`, `src/pages/settings.rs:279-291`, `src/components/modal_groups.rs:127-136`

- [ ] **BUG-29 — `trigger_refresh` has no debounce — stacks unbounded concurrent scans** *(B-L18)*
  Every request spawns fresh systemd + docker scans. Discovery panel fires it after every claim. Concurrent `retain`+`extend` writes can momentarily duplicate or drop list entries.
  *Files:* `src/discovery/mod.rs:74-95`

- [ ] **BUG-30 — Notification loop `prev_status` map never prunes deleted services** *(B-L19)*
  Entries persist for service IDs that no longer exist. Add a periodic `retain` against current IDs.
  *Files:* `src/notify.rs:26,94`

- [ ] **BUG-31 — Discovery/metrics/probe interval of `0` busy-loops** *(B-L20)*
  None of the loop intervals validate non-zero. A misconfigured `interval_secs = 0` spins the loop at full speed. Add validation in `AppConfig::load()`.
  *Files:* `src/discovery/systemd.rs:70-78`, `src/discovery/docker.rs:23-31`, `src/metrics/system.rs:79-93`, `src/probe/mod.rs:19`

- [ ] **BUG-32 — Probe-status fetch failure indistinguishable from "no data"** *(B-L21)*
  Error swallowed by `unwrap_or_default()` — DB error renders every service as "unknown" with no log.
  *Files:* `src/api/services.rs:71-82`

- [ ] **BUG-33 — Swagger UI and OpenAPI spec publicly exposed** *(B-L22)*
  `/swagger-ui` and `/api-docs/openapi.json` are outside auth layers, enumerating the full API surface to unauthenticated visitors. Gate or document this explicitly.
  *Files:* `src/api/mod.rs:45-47`

### Features

- [ ] **FEAT-9 — PAM auth: add role mapping (currently all PAM users are admin)** *(F-F4)*
  Add `auth.pam_admin_users` config allowlist so PAM deployments get the viewer/admin split.

- [ ] **FEAT-10 — Server-side user preference storage (theme, sidebar)** *(F-F11)*
  Preferences currently in `localStorage` only. `GET/PUT /api/v1/auth/me/prefs` storing a small JSON blob per user; frontend reads after login.

- [ ] **FEAT-11 — Group collapse + "problems first" dashboard strip** *(F-F12)*
  Collapsible group sections (collapse state in localStorage). Optional "attention" strip surfacing only `down` services. Pairs with FEAT-1.

- [ ] **FEAT-12 — Discord/Slack/ntfy webhook payload presets** *(F-F13)*
  Add `format` field per webhook (`generic | discord | slack | ntfy`) switching payload template in `fire_webhook`. Pairs with FEAT-6.

- [ ] **FEAT-13 — Prometheus-style `/metrics` text endpoint** *(F-F14)*
  `read_snapshot()` already gathers all needed data. Hand-render `vexboard_service_up{name=...}` etc. in Prometheus text format. No new dependencies.

---

## Summary

| Priority | Count |
|----------|-------|
| HIGH     | 8     |
| MEDIUM   | 26    |
| LOW      | 31    |
| **Total**| **65**|
