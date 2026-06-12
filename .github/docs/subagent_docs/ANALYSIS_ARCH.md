# VexBoard — Architecture & Structure Analysis

Scope: full read of both crates (`vexboard-server`, `vexboard-frontend`), migrations, config,
CI, Docker, and scripts. Findings are limited to architecture, structure, consistency,
abandonment, and dependencies — not feature requests. Line numbers refer to the current
working tree (commit `65378c2`).

Priority legend: **HIGH** = misleads users/operators or is a latent correctness/security hole;
**MEDIUM** = structural debt that actively costs maintenance effort; **LOW** = cosmetic or
contained.

---

## 1. Architectural anti-patterns / design problems

### A1. HIGH — Role changes never propagate to live sessions
- Files: `crates/vexboard-server/src/middleware/auth.rs:31-38`, `crates/vexboard-server/src/api/users.rs:274-295`, `crates/vexboard-server/src/api/auth.rs:185-190`
- Authorization is decided from the `role` string written into the session **at login time**.
  `update_user` changes the role in the `users` table but never touches the target user's
  sessions. Combined with A2 (sessions effectively never expire and survive restarts via the
  SQLite store), a demoted admin retains full admin power indefinitely until they voluntarily
  log out. The "last admin" and "self-demotion" guards in `users.rs` protect the DB state but
  not the live authorization state. The fix-shape is to either look up the role from the DB in
  `require_admin`, or delete/flag the target's sessions on role change.

### A2. HIGH — `auth.secret` and `auth.session_ttl_hours` are dead config; sessions never expire and the session table grows forever
- Files: `crates/vexboard-server/src/config.rs:40-41`, `crates/vexboard-server/src/main.rs:127-130`, `crates/vexboard-server/src/session_store.rs`
- `AuthConfig.secret` and `session_ttl_hours` are defined, documented in
  `config/default.toml:14-17` ("Session secret — override … in production", "168 # 7 days"),
  and even set by CI (`.github/workflows/ci.yml:46`) — but **nothing reads either value**
  (verified by grep; `session_ttl_hours` appears only in config.rs and tests.rs).
  `SessionManagerLayer::new(store).with_secure(...)` is built without `.with_expiry(...)`, so
  the cookie gets tower-sessions' default expiry, not the configured 7 days. Worse,
  `SqliteSessionStore` has no expired-row cleanup (load() filters on `expiry_date` but nothing
  ever `DELETE`s expired rows and no background sweep exists), so `tower_sessions` rows
  accumulate unboundedly. The config actively lies to the operator about security behavior.

### A3. HIGH — Per-service `probe_interval` is stored, editable, and completely ignored
- Files: `crates/vexboard-server/src/probe/mod.rs:19-48`, `crates/vexboard-server/src/db/models.rs:26`, `crates/vexboard-server/src/db/migrations/001_init.sql:23`
- The probe scheduler runs one global loop at `config.probe.default_interval_secs` and probes
  every enabled service each tick. The `probe_interval` column flows through the schema, the
  `Service` model, `CreateService`/`UpdateService` DTOs, the OpenAPI docs, and the frontend
  `EditFormData` — yet no code path ever reads `svc.probe_interval`. Users can set a value
  that has zero effect. Either honor it (per-service timers / next-due scheduling) or remove
  the field end-to-end.

### A4. MEDIUM — Probe execution model: unbounded task spawning, per-probe D-Bus connections, full-unit-list scans, per-probe HTTP clients
- Files: `crates/vexboard-server/src/probe/mod.rs:38-44`, `crates/vexboard-server/src/probe/uptime.rs:56-62, 195-206`
- Each tick spawns one detached `tokio::spawn` per service with no concurrency cap and no
  join; if probes are slower than the interval (e.g. 5s timeouts × many down services), tasks
  from successive ticks overlap on the same service. `probe_systemd_unit` →
  `unit_active_state` opens a **new D-Bus system connection and calls `ListUnits`** (the full
  unit table) per service per tick — O(services × all-units) every 30s, when systemd offers
  `GetUnit`/property reads for a single unit. `probe_service` builds a fresh
  `reqwest::Client` per probe instead of reusing one (the notification loop already
  demonstrates the shared-client pattern, `main.rs:118-121`).

### A5. MEDIUM — No data-access layer; raw SQL with hand-copied column lists scattered through handlers
- Files: `crates/vexboard-server/src/api/services.rs:51-54, 165-168, 245-248`, `crates/vexboard-server/src/probe/mod.rs:23-27`, plus all of `api/groups.rs`, `api/quick_links.rs`, `api/users.rs`, `api/setup.rs`
- The `db/` module pretends to be a data layer but contains exactly two things: a one-function
  `users.rs` and `audit.rs`. Everything else embeds SQL directly in HTTP handlers. The
  15-column `SELECT … FROM services` string is duplicated verbatim in four places; adding a
  column means hunting every copy. `api/users.rs` even does raw `sqlx::query_scalar` user
  lookups despite `db/users.rs` existing for exactly that purpose. Pick one convention —
  either commit to inline SQL or build out `db/` — currently it's both.

### A6. MEDIUM — `claim_service`: path parameter ignored, non-atomic check, and the frontend doesn't use it
- Files: `crates/vexboard-server/src/api/services.rs:501-547`, `crates/vexboard-frontend/src/components/discovery_panel.rs:94-121`
- `POST /services/{id}/claim` documents its own `id` as "unused; payload drives insert", does
  an EXISTS check then forwards to `create_service` (check-then-insert race; the DB has no
  UNIQUE constraint on `systemd_unit` to back it up). Meanwhile the discovery panel — the only
  UI for claiming — posts directly to `POST /api/v1/services`, bypassing the duplicate-unit
  guard entirely. The endpoint is maintained, OpenAPI-documented dead weight whose one
  protection isn't applied in the real flow.

### A7. MEDIUM — `X-Forwarded-For` trusted unconditionally; defeats login rate limiting
- Files: `crates/vexboard-server/src/api/auth.rs:24-35`, `crates/vexboard-server/src/rate_limit.rs`
- `client_ip` prefers the first `X-Forwarded-For` entry over the socket address with no
  "trusted proxy" gate. Any client connecting directly can send a fresh random XFF per request
  and (a) bypass the per-IP login rate limiter completely, and (b) forge the `ip_addr` written
  to the audit log. There should be a config flag (`behind_proxy = true/false`) deciding
  whether the header is honored.

### A8. MEDIUM — Audit log exposed to the `viewer` role
- File: `crates/vexboard-server/src/api/mod.rs:22-28`
- `/api/v1/audit` is nested under `viewer_protected`. Audit entries contain login-failure
  usernames and client IP addresses (`api/auth.rs:114-124`). Listing users is admin-only, but
  a viewer can reconstruct the user list (and watch admin activity, IPs, renames) from the
  audit feed. If intentional, it deserves a comment; it looks like an oversight given the
  users-endpoint policy.

### A9. MEDIUM — PAM mode grants every OS account full admin
- File: `crates/vexboard-server/src/api/auth.rs:84-112` (role hardcoded `"admin"`), `src/api/auth.rs:257-258`
- With `pam-auth` enabled, any user who can PAM-authenticate (i.e., any local account) gets
  the admin role; the role system built for local auth is bypassed entirely. There is no
  group membership or allowlist check. For a dashboard that can rename/delete services this
  is a coarse trust model and contradicts the role machinery that exists ten lines away.

### A10. LOW — Hand-rolled migration runner while sqlx's `migrate` feature is enabled and unused
- Files: `crates/vexboard-server/src/db/mod.rs:34-80`, `Cargo.toml:13` (`"migrate"` feature)
- Migrations are applied by `include_str!` + `pragma_table_info` existence probes, and the
  history is internally inconsistent: `001_init.sql:15` already creates `discovery_source`,
  yet `db/mod.rs:44-54` also carries an ALTER-TABLE backfill for it. `003`/`004` are gated by
  column checks in Rust rather than the migration files being self-describing. sqlx's
  embedded migrator (already compiled in via the feature flag) would replace all of this with
  a versioned `_sqlx_migrations` table.

### A11. LOW — Config and assets are CWD-relative; `"embedded"` sentinel is misleading
- Files: `crates/vexboard-server/src/config.rs:146-156`, `crates/vexboard-server/src/main.rs:134-143`
- `config::File::with_name("config/default")` resolves against the process CWD, and most
  config fields have no serde defaults, so the binary only starts when launched from a
  directory containing `config/default.toml` (or with a full set of env vars). The
  `assets_path = "embedded"` value doesn't embed anything — it silently means `./assets`
  (`main.rs:136-140`), which is only true inside the Docker image layout. Rename the sentinel
  or actually embed the dist output (e.g. `rust-embed`).

---

## 2. Structural inconsistencies

### S1. MEDIUM — Four different router-construction conventions across API modules
- Files: `crates/vexboard-server/src/api/mod.rs:20-50`, `api/setup.rs`, `src/discovery/mod.rs:39-43`
- `services`/`groups`/`quick_links` expose `read_router()` + `admin_router()`;
  `users`/`audit`/`metrics`/`auth` expose a single `router()`; `setup` exposes no router at
  all (its two routes are assembled inline in `api/mod.rs:41-42`); and `discovery`'s router
  lives outside the `api/` tree entirely (see S2). A new contributor cannot infer where a
  route is registered from the module name.

### S2. MEDIUM — `discovery` module mixes background infrastructure with HTTP handlers; `api/` boundary not respected
- Files: `crates/vexboard-server/src/discovery/mod.rs:39-116` (axum handlers + utoipa
  annotations), vs. `src/api/*` for every other endpoint
- All HTTP surface lives under `api/` except discovery's `list_discovered`/`trigger_refresh`,
  which sit next to the D-Bus/bollard scan loops. The OpenAPI doc has to reach out to
  `crate::discovery::*` (`api/openapi.rs:55-56`) breaking the otherwise uniform
  `crate::api::*` listing. Move the handlers to `api/discovery.rs` and leave the scanners
  where they are.

### S3. LOW — Frontend component naming mixes conventions
- Files: `crates/vexboard-frontend/src/components/` — `modal_edit.rs`, `modal_groups.rs`
  (prefix style) vs `quick_link_modal.rs` (suffix style); `status_badge.rs` exports a
  component named `StatusDot` (file/name mismatch).

### S4. LOW — `components/mod.rs` re-exports exactly one component
- File: `crates/vexboard-frontend/src/components/mod.rs:12`
- Only `UserMenu` gets a `pub use`; every other component is referenced by full path
  (`components::sidebar::Sidebar`, etc.). Pick one style.

### S5. LOW — `settings` table created and never referenced
- File: `crates/vexboard-server/src/db/migrations/001_init.sql:45-48`
- No code reads or writes the `settings` key/value table. Dead schema that every fresh
  database carries.

### S6. LOW — CI, preflight, and project docs contradict each other about the frontend crate
- Files: `.github/workflows/ci.yml:35,38` (`cargo clippy --workspace`,
  `cargo test --workspace`), `scripts/preflight.sh` ("frontend is wasm32-only; exclude it
  from native test runs" → `cargo test -p vexboard-server`), `CLAUDE.md` (forbids
  workspace-wide native builds outright)
- CI compiles and tests the whole workspace natively on every push — and apparently passes —
  while the local tooling and docs insist that is impossible. One of these is wrong; if the
  frontend does compile natively (Leptos CSR generally does), the preflight/doc constraint is
  stale and locally-run checks are weaker than CI for no reason. If it doesn't, CI is broken.

---

## 3. Inconsistent patterns

### I1. MEDIUM — Error handling: ad-hoc tuples everywhere, and DB failures silently coerced to "success-shaped" defaults in security-relevant paths
- Files: every handler in `crates/vexboard-server/src/api/`; specific coercions:
  `api/users.rs:96-100` (username-uniqueness check `unwrap_or(None)` → DB error reads as
  "name available"), `api/users.rs:234-238` (`admin_count` `unwrap_or(2)` → DB error reads
  as "more than one admin exists", weakening the last-admin guard),
  `api/services.rs:71-82` (probe map `unwrap_or_default()` → all services show "unknown"
  with no error surfaced), `src/discovery/docker.rs:176-183` (claimed-check
  `unwrap_or(false)` → DB error re-surfaces claimed containers)
- There is no shared error type or `IntoResponse` implementation; each handler hand-rolls
  `(StatusCode, Json(json!({"error": …})))` with slightly different wording. `thiserror` is
  in the dependency tree precisely for this and is never used (see D1). The bigger issue is
  the pattern split: some DB errors → 500, others are swallowed into defaults whose value
  changes behavior. The `unwrap_or(2)` cases at least fail safe; `unwrap_or(None)` in the
  uniqueness checks fails open.

### I2. MEDIUM — HTTP method and response-shape conventions vary per resource
- Files: `api/services.rs:34` / `api/groups.rs:22` / `api/quick_links.rs:22` (PUT used for
  merge-with-existing partial updates — PATCH semantics), vs `api/users.rs:20` and
  `api/auth.rs:21` (PATCH for the same kind of partial update); response bodies:
  `{"status":"updated"}` (services/groups/quick-links/users) vs `{"ok":true}`
  (`api/auth.rs:453`) vs `{"status":"ok"}` + HTTP 200 for a creation (`api/setup.rs:133`,
  every other create returns 201 + `{"id": …}`).
- A client SDK generated from the OpenAPI spec inherits all of this irregularity.

### I3. MEDIUM — Three different "clear this optional field" semantics inside the same update handler
- Files: `crates/vexboard-server/src/api/services.rs:271-303`, `api/quick_links.rs:134-144`, `api/groups.rs:164-167`
- In `UpdateService`: `description`/`url`/`icon` treat empty string as "set NULL";
  `group_id` uses `.or(existing)` so it **can never be cleared back to NULL** through the
  API (frontend sends `group_id: null` from the edit modal, which deserializes to `None`,
  which keeps the old group — assigning "No group" in the UI silently does nothing);
  `tags` keeps existing on `None` with no clear mechanism at all. `update_group` has the
  same un-clearable `icon`/`color` (`.or(existing)`), while `update_quick_link` uses the
  empty-string-clears convention. Same problem, three answers, one of them a live UI bug.

### I4. MEDIUM — Frontend has no API client layer; DTOs and fetches duplicated per file
- Files: four distinct "group" types — `GroupResponse`
  (`pages/dashboard/mod.rs:40-45`), `GroupItem` (`components/modal_edit.rs:20-24`),
  `GroupEntryFe` (`components/discovery_panel.rs:55-59`), `GroupEntry`
  (`components/modal_groups.rs:18-26`) — with `/api/v1/groups` independently fetched in
  three places; `UserRecord` (`pages/settings.rs:7-12`) duplicates `UserPublic`;
  `MeResponse` (`components/user_menu.rs:46-66`) refetches `/api/v1/auth/me` even though
  `MainLayout` (`src/main.rs:94-116`) already fetched it into the `CurrentUser` context —
  two requests and two sources of truth for identity on every page load. The
  `is_admin` closure is copy-pasted in four files (`dashboard/mod.rs:73-78`,
  `service_grid.rs:23-28`, `quick_links_section.rs:17-22`, `settings.rs:22-27`).
- A single `api.rs` module with shared response structs would eliminate all of this and
  remove the risk of struct fields drifting from the server models they shadow.

### I5. MEDIUM — Large-scale copy-paste inside and across frontend components
- Files: `pages/dashboard/service_grid.rs` — the ~70-line draggable card wrapper +
  section-header + drop-handler block appears three times nearly verbatim (lines 162-231,
  304-373, 420-473), and the reset-to-A-Z payload/button appears three times;
  `extract_favicon_url` duplicated character-for-character (`components/modal_edit.rs:4-18`
  and `components/quick_link_modal.rs:12-26`); the icon letter/img-fallback block and the
  Edit/Remove button group duplicated between `service_card.rs` and `quick_link_card.rs`;
  the user-list refresh fetch is inlined three times in `pages/settings.rs` (lines 262-267,
  283-289, 363-368).
- 484-line `service_grid.rs` would drop to roughly a third with one `DraggableCard`
  component and one `SectionHeader` component.

### I6. LOW — Three styling mechanisms used interchangeably
- Files: `crates/vexboard-frontend/style/main.css` (565 lines of classes) vs massive inline
  `style="…"` strings throughout `service_grid.rs`/`dashboard/mod.rs`/`discovery_panel.rs`
  vs raw JS-string event attributes for hover states (`onmouseover="this.style.color=…"` in
  `service_card.rs:156-157, 175-176`, `quick_link_card.rs:43-44, 87-88`,
  `dashboard/mod.rs:152-153, 211-212`, `settings.rs:277-278`) alongside idiomatic reactive
  `style=move ||` closures elsewhere. The JS-string hovers are exactly what the CSS
  `:hover` classes in `main.css` already do.

### I7. LOW — Backend declares the same systemd D-Bus proxy twice; port-selection algorithm implemented twice
- Files: `src/discovery/systemd.rs:11-33` (`ManagerProxy` + `UnitInfo`) vs
  `src/probe/uptime.rs:10-32` (`SystemdManagerProxy` + `SystemdUnitInfo`) — identical
  interface, identical ten-field struct, two names. The three-tier "preferred / port-80 /
  any" port-selection heuristic is implemented once over bollard port structs
  (`src/discovery/docker.rs:189-228`) and once over `docker port` text output
  (`src/discovery/systemd.rs:313-359`); a behavior change (e.g. the recent "prefer 81 over
  8444" fix) must be made twice and tested twice — the tests at `systemd.rs:744-793` cover
  only one copy.

### I8. LOW — Tracing instrumentation applied inconsistently
- Files: every handler in `api/services.rs`, `api/users.rs`, `api/groups.rs`, `api/auth.rs`
  carries `#[tracing::instrument]`; none of the five handlers in `api/quick_links.rs` do.
  Log message style also varies ("Failed to list services" / "DB error" / lowercase
  "failed to persist session…").

---

## 4. Half-implemented or abandoned

### H1. HIGH — Editing a service silently resets its probe settings
- Files: `crates/vexboard-frontend/src/components/modal_edit.rs:188-197` (hardcodes
  `probe_enabled: true, probe_interval: 30` on save), `modal_edit.rs:27-35` (`EditFormData`
  carries both fields), `pages/dashboard/service_grid.rs:49-57` (edit flow populates them
  from the real service)
- The data is threaded all the way into the modal, but the modal renders no inputs for it
  and discards the initial values on save. Any edit to a service with probing disabled
  re-enables probing; any custom interval is reset to 30. This is a live data-loss bug
  produced by an unfinished form, compounding A3.

### H2. MEDIUM — `tags` feature is a stub
- Files: `db/migrations/001_init.sql:24`, `db/models.rs:27,65,80`,
  `api/services.rs:120-132, 291-303`
- Column, DTO fields, JSON serialization handling, and OpenAPI schema all exist; no frontend
  surface reads or writes tags, and the update path provides no way to clear them. Either
  finish or strip it.

### H3. MEDIUM — `visible` flag has no management surface; setting it hides a service permanently
- Files: `api/services.rs:54` (`WHERE visible = 1`), `db/models.rs:28,66,81`
- The only list endpoint filters invisible services out, and there is no
  "show hidden"/admin listing or UI toggle. A service set `visible=false` via the API
  disappears from every UI including the edit path. Half a feature with a trapdoor.

### H4. MEDIUM — Real-time service-status streaming is plumbed but never exposed
- Files: `src/main.rs:39,67` (`probe_tx` in `AppState`), `api/services.rs:159-189`
  (immediate probe on create feeds the channel), `api/metrics.rs:25` (docstring claims the
  SSE endpoint streams "live system metrics **and service status events**" — it only sends
  `system` events), `pages/dashboard/modals.rs:38-41` (frontend compensates with a
  hardcoded 1.5-second sleep-then-refetch)
- The broadcast channel for `ProbeEvent` reaches the API layer and the notification loop
  subscribes to it, but no SSE/endpoint ever delivers probe events to the browser. The
  dashboard's status dots only update on full refetch. The misleading docstring suggests
  this was planned and dropped midway.

### H5. MEDIUM — Claim endpoint orphaned by its own UI
- See A6: `POST /services/{id}/claim` is implemented, tested by nothing, documented in
  OpenAPI, and bypassed by the discovery panel which posts to `/services` directly.

### H6. LOW — `DiscoveredPage` wires a required callback to a no-op; discovery fields shipped but unused
- Files: `crates/vexboard-frontend/src/pages/discovered.rs:15`
  (`on_added=Callback::new(move |_| {})`), `src/discovery/mod.rs:22-31` (`active_state`,
  `sub_state` serialized to the client, never rendered)
- The `on_added` prop exists to let a parent refresh after a claim, but the only caller
  passes an empty closure — either the prop or the page integration is unfinished.

### H7. LOW — No frontend consumes the audit API
- Files: `api/audit.rs` (full paginated endpoint, viewer-accessible), no reference to
  `/api/v1/audit` anywhere in `vexboard-frontend/src`
- Audit infrastructure (table, indexes, insert calls in every handler, list endpoint) is
  complete on the backend and invisible in the product.

### H8. LOW — Overlapping user DTOs from different eras
- Files: `db/models.rs:106-111` (`UserInfo` — used in exactly one place, the local-login
  response), `db/models.rs:43-49` (`UserPublic`), `api/auth.rs:271-277` (`/me` builds its
  user JSON by hand instead of using either); `SetupRequest` lives in `api/setup.rs:8-12`
  while every other request DTO lives in `db/models.rs`.

---

## 5. Dependencies — unnecessary, misused, or outdated

### D1. MEDIUM — `thiserror` declared and never used
- Files: `Cargo.toml:23` (workspace), `crates/vexboard-server/Cargo.toml:32`
- No `#[derive(Error)]` or `thiserror::` reference exists in the server crate (verified by
  grep). It was presumably added for the shared API error type that never materialized
  (see I1). Use it or remove it.

### D2. LOW — `js-sys` declared in the frontend and never used
- File: `crates/vexboard-frontend/Cargo.toml:16`
- No `js_sys::` reference anywhere in `vexboard-frontend/src`. (It will be in the tree
  transitively via wasm-bindgen regardless, but the direct dependency is noise.)

### D3. LOW — `tower` is a runtime dependency but only used by tests
- Files: `crates/vexboard-server/Cargo.toml:33`, `src/tests.rs:11` (`tower::ServiceExt` —
  the only use)
- Should be moved to `[dev-dependencies]`; production code uses `tower-http` and
  `axum::middleware` only.

### D4. LOW — `bollard` pinned at 0.17 (resolved 0.17.1) using the legacy options API
- Files: `Cargo.toml:24`, `src/discovery/docker.rs:4,104`
  (`ListContainersOptions::<String>`)
- bollard 0.18+ moved to a query-builder options API and deprecated the typed-string
  options structs in use here. Not broken, but already one major version of churn behind,
  and the upgrade gets harder as the discovery code grows.

### D5. LOW — Two datetime libraries (`chrono` + `time`)
- Files: `Cargo.toml:21`, `crates/vexboard-server/Cargo.toml:28`,
  `src/session_store.rs:5` (`time`, forced by tower-sessions' `Record`),
  `src/db/models.rs:1` and `src/notify.rs:54` (`chrono`)
- Justified by tower-sessions' API, so not removable today — but worth knowing the split is
  deliberate, and any future timestamp work should not pick a third convention
  (`probe_results.checked_at` and friends are currently strings/`NaiveDateTime` via
  sqlite's `CURRENT_TIMESTAMP`, i.e. naive local-vs-UTC ambiguity already exists at the
  schema level).

### D6. LOW — sqlx `migrate` feature compiled in and unused
- File: `Cargo.toml:13` — see A10. Either adopt `sqlx::migrate!` or drop the feature to
  shave compile time.

---

## Summary counts

| Priority | Count |
|----------|-------|
| HIGH     | 5 (A1, A2, A3, H1 + the A6/H5 pair if claim-flow integrity matters to you) |
| MEDIUM   | 15 |
| LOW      | 16 |

The three themes that generate most of the findings:

1. **Config/schema promises the code doesn't keep** — `secret`, `session_ttl_hours`,
   `probe_interval`, `tags`, `visible`, the `settings` table, the "embedded" assets
   sentinel. Each one misleads an operator or user.
2. **No shared layers where the codebase clearly wants them** — no API error type
   (backend), no data-access layer (backend), no API client/DTO module (frontend). Almost
   every MEDIUM consistency finding is a downstream symptom of these three gaps.
3. **Session-state authorization** — A1 + A2 together mean role enforcement is only as
   fresh as the login that minted the session, and sessions are immortal. This is the one
   cluster worth fixing before anything else.
