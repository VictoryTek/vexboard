# VexBoard — Feature Opportunity Analysis

Scope: full read of both crates (`vexboard-server`, `vexboard-frontend`), migrations, config,
and frontend pages/components. Findings are limited to features that fit the existing
architecture (Axum + SQLite + SSE backend, Leptos CSR frontend) — nothing here requires a
rearchitecture. Line numbers refer to the current working tree (commit `26e57db`).

Companion documents: `ANALYSIS_ARCH.md` (structural debt), `ANALYSIS_BUGS.md` (defects).
Where a finding overlaps (e.g. the ignored `probe_interval`), it is listed here only because
finishing it is a user-facing feature, not just a cleanup.

Priority legend: **HIGH** = high value relative to effort, builds directly on existing
plumbing; **MEDIUM** = clear value, moderate effort; **LOW** = nice-to-have or niche.

---

## 1. Partially stubbed / clearly intended but unfinished

### F1. HIGH — Live service-status updates over SSE (the probe event bus already exists)
**What exists:** The backend already has a complete pub/sub pipeline for probe results:
`probe_tx: broadcast::Sender<ProbeEvent>` lives in `AppState` (`main.rs:39`), every probe
broadcasts a serializable `ProbeEvent { service_id, service_name, url, status, latency_ms }`
(`probe/uptime.rs:35-42, 120-129`), and the SSE machinery is proven in
`api/metrics.rs:38-52` (`metrics_stream` wraps a broadcast receiver in `BroadcastStream`).
The frontend already opens an `EventSource` for metrics (`components/metric_bar.rs:69-72`).
Yet **no API handler ever subscribes to `probe_tx`** — only the webhook notifier consumes it
(`main.rs:117`). The frontend compensates by refetching the whole service list after every
mutation, including a hard-coded "wait briefly then refetch so the immediate probe lands"
hack (`pages/dashboard/modals.rs:37-41`).

**Concrete feature:** Add `GET /api/v1/services/stream` (viewer-protected, same shape as
`metrics_stream`) that forwards `ProbeEvent`s as `event: service` SSE messages. In
`service_grid.rs`, subscribe once and patch the matching card's status/latency signal instead
of refetching. Status changes (up→down) appear on the dashboard within one probe cycle with
zero polling.

**Why this priority:** ~50 lines of backend code reusing two existing patterns; removes the
sleep-hack; turns the dashboard from "eventually consistent on manual refresh" into the
real-time board the SSE architecture was clearly built for.

### F2. HIGH — Dismiss/ignore for discovered services (the UI already claims it exists)
**What exists:** The Settings page tells users: *"Discovered services appear in the dashboard
for you to claim or dismiss"* (`pages/settings.rs:179-182`). Claiming is implemented
(`api/services.rs:519-547` `claim_service`, plus the discovery panel UI) — **dismiss does not
exist anywhere** (no handler, no UI button, no persistence; verified by grep across both
crates). Every unclaimed unit reappears in the discovery panel on every scan, forever. The
config offers only global static `exclude_units` patterns in TOML (`config/default.toml`),
which requires a restart and can't be driven from the UI.

**Concrete feature:** A `dismissed_units` table (`source`, `unit_name`, `created_at`) or rows
in the unused `settings` table; `POST /api/v1/discovery/dismiss` +
`DELETE /api/v1/discovery/dismiss/{...}` (admin); discovery loops filter dismissed names when
publishing to `DiscoveryList` (one extra `HashSet` check in
`discovery/systemd.rs` / `discovery/docker.rs:70-74`); a "Dismiss" button next to "Claim" in
`components/discovery_panel.rs`, and a "show dismissed" toggle to undo.

**Why this priority:** The product copy already promises it; the discovery list is otherwise
permanently noisy on real servers, which undermines the core discovery feature.

### F3. MEDIUM — Honor per-service `probe_interval` (stored, editable, ignored)
**What exists:** `probe_interval` flows through the schema (`001_init.sql:23`), the `Service`
model (`db/models.rs:26`), both DTOs, the OpenAPI docs, and the create/update handlers — but
the scheduler runs one global loop at `config.probe.default_interval_secs` and probes every
service each tick (`probe/mod.rs:20-49`); nothing ever reads `svc.probe_interval`. (Also
flagged as ANALYSIS_ARCH A3.)

**Concrete feature:** Next-due scheduling: keep a `HashMap<i64, Instant>` of last-probed
times in the loop, tick at a short base interval (e.g. 5s), and only probe services whose
`probe_interval` has elapsed. No schema or API change needed — the field already round-trips.

**Why this priority:** Small, contained change to one loop; makes an existing, documented,
user-editable field actually do something. (The alternative — removing the field end-to-end —
is a cleanup, not a feature; honoring it is the better fit since the UI exposes it.)

### F4. LOW — Finish PAM auth mode (currently second-class by design gaps, not by intent)
**What exists:** A whole `pam_auth.rs` module behind the `pam-auth` feature, a deliberate
405-returning `update_me` stub for PAM mode (`api/auth.rs:285-311` — the comment documents
the stub explicitly), and `auth_mode` surfaced in `/auth/me`. But in PAM mode every user is
hard-coded `"admin"` (`api/auth.rs:258`), so the viewer/admin role system (migration 003, all
the `require_admin` middleware) is bypassed entirely.

**Concrete feature:** Map PAM users to roles — e.g. a config list `auth.pam_admin_users` or
group-membership check — so PAM deployments get the same viewer/admin split as local auth.

**Why this priority:** Only matters to the subset of deployments compiling with `pam-auth`;
the stub is functional today, just coarse.

---

## 2. Natural complements to existing code and data models

### F5. HIGH — Uptime history endpoint + sparkline/uptime-% on service cards
**What exists:** The probe system already persists up to `max_history = 100` results per
service with status, latency, and timestamp (`probe_results` table; trimming logic in
`probe/uptime.rs:106-117`), and the config documents this retention
(`config/default.toml: [probe] max_history = 100`). But the **only** read of that table is
"latest row per service" (`api/services.rs:71-82`). The history is collected, trimmed,
maintained — and never shown to anyone.

**Concrete feature:** `GET /api/v1/services/{id}/history?limit=100` returning
`[{status, latency_ms, checked_at}]` (viewer-protected, mirrors `api/audit.rs` pagination
style). Frontend: a latency sparkline + "uptime % over last N checks" strip on
`components/service_card.rs` (or in an expanded card view). This is the Uptime-Kuma-style
view users expect from anything that advertises "Uptime Probing — HTTP health checks with
latency tracking" (README features list).

**Why this priority:** The expensive part (collection, retention, trimming) is done; the
feature is one read endpoint and one small chart component.

### F6. MEDIUM — Tags: surface them in the UI (filter chips + search)
**What exists:** `tags` is a first-class column (`001_init.sql:24`), serialized as JSON
through `CreateService`/`UpdateService` (`api/services.rs:120-132, 291-303`), present in the
OpenAPI schema — and **completely absent from the frontend**: no input field in
`modal_edit.rs`, no display on cards, no filtering (verified by grep — zero occurrences of
"tags" in `vexboard-frontend/src`). There is also no search/filter UI of any kind on the
dashboard.

**Concrete feature:** (a) tags input in the edit modal (comma-separated → `Vec<String>`),
(b) tag chips on `ServiceCard`, (c) a filter bar above the service grid: free-text search on
name/description plus clickable tag chips — all client-side over the already-fetched service
list, no backend change at all.

**Why this priority:** Backend is 100% done; pure frontend work. Becomes important as soon
as a server has more than ~20 services, which discovery makes likely.

### F7. MEDIUM — Audit log viewer page
**What exists:** A full audit pipeline: `audit_log` table with indexes (`002_audit_log.sql`),
`db::audit::insert` called from every mutating handler (services, groups, users, discovery
refresh…), and a paginated, viewer-accessible read API (`api/audit.rs:44-94`, mounted at
`/api/v1/audit` in `api/mod.rs`). The frontend never calls it — zero references to "audit" in
`vexboard-frontend/src`.

**Concrete feature:** An "Audit Log" page (sidebar entry, admin-or-viewer) rendering the
paginated table: time, actor, action, resource, detail. The API's `limit`/`offset` params are
already built for exactly this. Optionally a "Recent activity" card on Settings.

**Why this priority:** The whole backend exists and is already paying the write cost on every
mutation; the data is invisible until someone curls the API.

### F8. MEDIUM — Webhook management via API/UI (backed by the dormant `settings` table)
**What exists:** Two ready-made halves that were never joined: (1) a complete webhook
delivery engine with HMAC signing, retries, and per-hook event filters
(`notify.rs`, `config/default.toml [notifications]`) — but hooks are **TOML-only**, so adding
or changing one requires editing a file and restarting; (2) a `settings` key-value table
created in `001_init.sql` that **no code reads or writes** (verified by grep — zero
references outside the migration).

**Concrete feature:** Store webhooks as rows (in `settings` as JSON values, or a small
`webhooks` table), expose admin CRUD at `/api/v1/notifications/webhooks`, add a
"Notifications" card on the Settings page (URL, events, secret, a "send test event" button),
and have `notification_loop` read from the DB (or receive a reload signal through a
`watch` channel) instead of a frozen config clone.

**Why this priority:** Turns a hidden, restart-gated feature into a usable one; the delivery
machinery — the hard part — already exists and is tested in production shape.

### F9. MEDIUM — Service control actions (start / stop / restart)
**What exists:** Both control planes are already wired into the binary: a zbus systemd
`Manager` proxy (`probe/uptime.rs:10-17` — currently exposing only `list_units`, but
`StartUnit`/`StopUnit`/`RestartUnit` live on the same interface) and bollard Docker/Podman
connections (`discovery/docker.rs:99` — `restart_container` etc. are on the same `Docker`
handle). Services already record their origin (`systemd_unit`, `discovery_source`), the
admin/viewer role split exists to gate it, and the audit log exists to record it.

**Concrete feature:** `POST /api/v1/services/{id}/action` with body `{"action": "restart"}`
(admin-only, audited, allowlist of `start|stop|restart`), dispatching to zbus or bollard
based on `discovery_source`/`systemd_unit`. Frontend: a restart button on the service card's
admin menu with a confirm step. Trigger an immediate re-probe afterwards (the pattern already
exists in `create_service`, `api/services.rs:159-189`).

**Why this priority:** High user value (it's the #1 thing dashboards like this get asked
for), moderate effort, and every dependency is already linked in. Priced as MEDIUM rather
than HIGH only because it's security-sensitive and needs careful permission/confirmation
design — note the NixOS module's hardening would also need `vexboard` to gain polkit/D-Bus
rights for unit control.

---

## 3. Obvious user-expectation gaps

### F10. MEDIUM — Export / import of dashboard configuration
**What exists:** Every entity (`Service`, `Group`, `QuickLink`) derives both `Serialize` and
`Deserialize` (`db/models.rs`), and the whole dashboard state is four small SQLite tables.
There is no backup/restore or migration story at all (zero matches for export/import/backup
in either crate) — losing `vexboard.db` means manually re-claiming and re-decorating every
service. Homepage/Dashy users expect their layout to be portable.

**Concrete feature:** `GET /api/v1/export` returning
`{groups, services, quick_links, version}` as a JSON document (admin-only), and
`POST /api/v1/import` with merge/replace semantics matching on `systemd_unit`/name. A
Download/Upload pair on the Settings page. Probe history and users deliberately excluded.

**Why this priority:** Cheap (serde does the work), and it's the standard insurance feature
for "pet" dashboards that accumulate manual curation.

### F11. LOW — Per-user/server-side preference storage (theme, sidebar) using `settings`
**What exists:** Theme and sidebar mode persist only in browser `localStorage`
(`pages/settings.rs:96-111`, `components/sidebar.rs`), so preferences reset on every new
browser/device. The unused `settings` table (or a `user_prefs` column) is sitting there.

**Concrete feature:** `GET/PUT /api/v1/auth/me/prefs` storing a small JSON blob per user;
frontend reads it after login and falls back to localStorage.

**Why this priority:** Genuine polish, but localStorage already covers the single-browser
case; only worth doing after F8 gives the `settings` table (or equivalent) a real owner.

### F12. LOW — Group collapse + "problems first" dashboard ordering
**What exists:** Groups with colors and sort order (migrations 001/004), a grouped grid
(`pages/dashboard/service_grid.rs:116-140`), and per-card status. With many services, users
expect to collapse healthy groups and see down services surfaced.

**Concrete feature:** Collapsible group sections (collapse state in localStorage), and an
optional "attention" strip at the top of the dashboard listing only services currently
`down`. Pure frontend; pairs naturally with F1's live updates.

**Why this priority:** Small quality-of-life win; value scales with fleet size.

---

## 4. Integrations/automations the structure is ready for but not using

### F13. LOW — Richer webhook targets (Discord/Slack/ntfy payload presets)
**What exists:** The generic JSON webhook with HMAC (`notify.rs:46-57`) — but Discord, Slack,
and ntfy each reject or mangle the generic payload, so the most common homelab notification
targets need a translation layer the user currently has to host themselves.

**Concrete feature:** A `format` field per webhook (`generic | discord | slack | ntfy`) that
switches the payload template in `fire_webhook`. ~80 lines, no new dependencies. Best done
together with F8 so the format is selectable in the UI.

### F14. LOW — Prometheus-style `/metrics` text endpoint
**What exists:** `read_snapshot()` already gathers CPU/mem/net/disk on demand
(`metrics/system.rs:22-47`), and per-service up/down + latency is one query away
(`api/services.rs:71-82`). Self-hosters running Prometheus/Grafana would scrape this.

**Concrete feature:** An unauthenticated-or-token-gated `GET /metrics` rendering
`vexboard_service_up{name=...}`, `vexboard_service_latency_ms{...}`, and the system gauges in
the Prometheus text format — hand-rendered strings, no new crate required.

**Why this priority:** Niche overlap with existing SSE metrics; valuable only to the
monitoring-stack crowd, but nearly free given the collectors exist.

---

## Suggested build order

| # | Feature | Priority | Effort | Notes |
|---|---------|----------|--------|-------|
| F1 | Live status SSE stream | HIGH | S | Reuses metrics SSE + probe bus |
| F5 | Uptime history API + sparkline | HIGH | S–M | Data already collected |
| F2 | Dismiss discovered services | HIGH | S–M | UI copy already promises it |
| F6 | Tags UI + search/filter | MEDIUM | S (frontend only) | Backend done |
| F7 | Audit log viewer page | MEDIUM | S (frontend only) | Backend done |
| F3 | Honor `probe_interval` | MEDIUM | S | One loop change |
| F8 | Webhook management UI/API | MEDIUM | M | Activates `settings` table |
| F9 | Service start/stop/restart | MEDIUM | M | Security design needed |
| F10 | Export / import | MEDIUM | S–M | serde does the work |
| F11 | Server-side user prefs | LOW | S | After F8 |
| F12 | Group collapse / problems-first | LOW | S | Pure frontend |
| F13 | Discord/Slack/ntfy presets | LOW | S | Pairs with F8 |
| F4 | PAM role mapping | LOW | M | Feature-gated audience |
| F14 | Prometheus `/metrics` | LOW | S | Niche |
