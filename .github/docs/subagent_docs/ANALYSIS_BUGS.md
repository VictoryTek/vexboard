# VexBoard — Bug & Code Quality Analysis

Date: 2026-06-11
Scope: full codebase review (`crates/vexboard-server`, `crates/vexboard-frontend`, `config/`, `scripts/`, `nix/`) for logic errors, security issues, performance problems, dead code, and error-handling gaps. Static review only; no forbidden build commands were run.

Priority legend:
- **HIGH** — security impact, data corruption, or a user-visible feature that silently does the wrong thing.
- **MEDIUM** — incorrect behavior in edge cases, fail-open error handling, meaningful performance waste.
- **LOW** — dead code, doc/code mismatch, minor inefficiency, cosmetic logic flaws.

---

## HIGH

### H1. `X-Forwarded-For` is trusted unconditionally — login rate limit is trivially bypassable
**Files:** `crates/vexboard-server/src/api/auth.rs:25-35`, `crates/vexboard-server/src/rate_limit.rs:25-38`

`client_ip()` prefers the **first** entry of the client-supplied `X-Forwarded-For` header over the real socket address, with no concept of a trusted proxy:

- Any unauthenticated attacker can send a different fake IP per request and brute-force passwords with **no effective rate limit** (each spoofed IP gets a fresh attempt budget).
- Each spoofed IP inserts a new entry into the `LoginRateLimiter` HashMap, which is **never evicted** (entries persist for the life of the process, even after the window empties). Spoofing millions of random IPs grows memory unboundedly — a slow memory-exhaustion DoS on the login endpoint.
- The spoofed IP is also written to the audit log (`auth.login_failure` / `auth.login_success`), so the audit trail for break-in attempts is attacker-controlled.

Even when a reverse proxy *is* in front, taking the *first* XFF entry is wrong — that field is set by the original client; only the last entry appended by your own proxy is trustworthy. Fix: only honor XFF (last hop) when explicitly enabled via config, otherwise use `connect_info`, and evict empty `VecDeque`s from the map.

### H2. Role/permission changes and user deletion do not invalidate existing sessions
**Files:** `crates/vexboard-server/src/middleware/auth.rs:20-38`, `crates/vexboard-server/src/api/users.rs:182-305` (update), `:325-413` (delete)

`require_admin` reads `role` from the **session record**, which is written once at login and never re-validated against the database:

- Demoting an admin to viewer (`PATCH /api/v1/users/{id}`) has **no effect on their live sessions** — they keep full admin access until they log out voluntarily or the session record expires (and see H4: no TTL is configured, so this is the library default, not your configured 7 days).
- **Deleting a user does not delete their sessions.** `require_auth` only checks that `username` exists in the session, never that the user still exists in `users`. A deleted user retains full (possibly admin) access with their existing cookie.
- Renaming a user leaves the old username in their session, so subsequent audit entries attribute their actions to a username that no longer exists, and the self-demotion guard (`target.username == actor`, users.rs:220) silently stops matching.

Fix: on role change / rename / delete, delete the target's rows from `tower_sessions` (requires indexing sessions by username or scanning the JSON), or re-check role against the DB in `require_admin`.

### H3. Edit Service modal silently resets probing config on every save
**File:** `crates/vexboard-frontend/src/components/modal_edit.rs:188-197` (with `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs:49-57`)

The modal receives the service's real `probe_enabled` / `probe_interval` in `initial`, but the Save handler hardcodes them:

```rust
on_save.run(EditFormData {
    ...
    probe_enabled: true,      // always true
    probe_interval: 30,       // always 30
});
```

The PUT body then overwrites the DB values (`api/services.rs:288-289` applies them since they are `Some`). Editing *anything* about a service (rename, change icon, etc.) silently re-enables probing on a service the user disabled and resets a custom interval to 30. There is no probing UI in the modal at all, so the user cannot even see this happening.

### H4. `auth.secret` and `auth.session_ttl_hours` are dead config — the NixOS module hard-gates startup on a value that does nothing
**Files:** `crates/vexboard-server/src/config.rs:40-41`, `crates/vexboard-server/src/main.rs:129-130`, `nix/module.nix:68-91,132-150`, `config/default.toml:15-17`

`AppConfig.auth.secret` and `session_ttl_hours` are deserialized and **never read anywhere** (verified by grep over the crate). Consequences:

- `SessionManagerLayer::new(store)` is built without `.with_expiry(...)`, so the documented `session_ttl_hours = 168  # 7 days` has **no effect**. Sessions live for tower-sessions' built-in default, not your configured TTL, and the cookie is a browser-session cookie. Combined with H2 this means revocation lag is unbounded by your own config.
- `nix/module.nix` refuses to start the service (`preStart` exits 1) unless `VEXBOARD_AUTH__SECRET` is set to a non-default value — an entire `secretFile` workflow (generate with openssl, chmod 0400, etc.) enforcing a secret **the server never uses**. This is security theater that misleads operators into believing sessions are keyed to that secret.

Either wire the secret/TTL into the session layer (e.g. signed cookies + `Expiry::OnInactivity(hours)`) or delete the options and the Nix gating.

### H5. Claimed Docker/Podman containers are probed as systemd units and always report "down"
**Files:** `crates/vexboard-frontend/src/components/discovery_panel.rs:98-99`, `crates/vexboard-server/src/probe/mod.rs:38-44`, `crates/vexboard-server/src/probe/uptime.rs:195-206`

When the user clicks "Add" on a **docker/podman** discovery, the frontend posts:

```rust
"systemd_unit": unit_name,        // = container name, e.g. "nginx-proxy-manager"
"discovery_source": source,       // "docker" / "podman"
```

The probe scheduler gives `systemd_unit` priority over `url` (`probe/mod.rs:39`), so the service is probed via `unit_active_state()`, which looks for a systemd unit literally named `nginx-proxy-manager` (no `.service` suffix, not a unit at all), never finds it, and returns `"inactive"` → status **"down" forever**, even though the container is running and the URL probe would succeed. The dashboard shows a permanently red dot for every claimed container.

Fix: only set `systemd_unit` when `source == "systemd"`, or make the probe dispatcher respect `discovery_source`.

### H6. Role dropdown in "Add User" is dead — selected role is never read
**File:** `crates/vexboard-frontend/src/pages/settings.rs:330-342`

The `<select>`'s `on:change` handler does:

```rust
t.dyn_into::<web_sys::HtmlInputElement>()
```

A `<select>` element is an `HtmlSelectElement`, not an `HtmlInputElement`, so `dyn_into` **always fails** and the closure silently does nothing. `new_role` stays at its initial `"viewer"`, so choosing "Admin" in the dropdown is ignored and every user created through the Settings page is a viewer. (`web-sys` in `crates/vexboard-frontend/Cargo.toml:15` doesn't even enable the `HtmlSelectElement` feature.) The only workaround is the separate "→ Admin" toggle after creation, which masks the bug.

---

## MEDIUM

### M1. No session ID rotation on login (session fixation)
**File:** `crates/vexboard-server/src/api/auth.rs:84-130` (PAM), `:132-211` (local)

Neither login path calls `session.cycle_id()` before inserting `username`/`role`. The pre-authentication session ID (which an attacker may have planted, e.g. via a subdomain cookie injection) remains valid after privilege elevation. tower-sessions provides `cycle_id()` exactly for this; call it on successful login.

### M2. Login rate limiter can panic during the first window after boot
**File:** `crates/vexboard-server/src/rate_limit.rs:27`

```rust
let cutoff = now - self.window;
```

`Instant - Duration` **panics** on underflow. On Linux, `Instant` is `CLOCK_MONOTONIC`, whose origin is boot time. VexBoard is deployed as a boot-started systemd service (`nix/module.nix:130-131`); any login attempt made less than `login_rate_limit_window_secs` (60 s default) after boot panics the handler task and the request fails. Use `now.checked_sub(self.window)` and treat `None` as "nothing is old enough to evict".

### M3. `me()` defaults a missing role to `"admin"`
**File:** `crates/vexboard-server/src/api/auth.rs:260-268`

```rust
.unwrap_or_else(|| "admin".to_string())
```

If the `role` key is absent from a session — plausible, because sessions persist in SQLite across upgrades (a session created by a pre-roles build has no `role` key) — `/api/v1/auth/me` reports `role: "admin"`. The frontend then renders the full admin UI (`main.rs` → `CurrentUser::is_admin`). `require_admin` does *not* share the fallback so writes are still rejected, but a viewer-grade session is shown admin controls and "succeeds" silently-failing actions. Fail-safe default should be `"viewer"`.

### M4. Last-admin guards fail open on DB error
**File:** `crates/vexboard-server/src/api/users.rs:234-238` and `:369-373`

```rust
.unwrap_or(2);
if admin_count <= 1 { ... }
```

If the `COUNT(*)` query errors, the code assumes 2 admins and **allows** demoting/deleting what may be the last admin, potentially locking everyone out of administration. Fail closed (return 500) on a count failure.

### M5. `update_service` can never clear `group_id` (or `discovery_source`) — "No group" in the edit modal is a no-op
**Files:** `crates/vexboard-server/src/api/services.rs:272,286`, `crates/vexboard-frontend/src/pages/dashboard/modals.rs:92-100`

The PUT merge uses `payload.group_id.or(existing.group_id)`, so a JSON `null` (what the modal sends when "— No group —" is selected, modal_edit.rs:156-157) means "keep existing". There is **no representation for "set NULL"**: a service can never be un-grouped through the API, and the edit modal's "No group" option silently does nothing. Same pattern blocks clearing `discovery_source` (services.rs:272) and a group's `icon`/`color` (`api/groups.rs:165-166`). The codebase already has an empty-string-clears convention for description/url/icon — group_id needs an equivalent (e.g. accept `0` or a sentinel, or use a double-Option).

### M6. Per-service `probe_interval` is ignored — all services probe at the global interval
**File:** `crates/vexboard-server/src/probe/mod.rs:19-48`

The scheduler sleeps `config.default_interval_secs` and probes **every** enabled service each pass. The `probe_interval` column (schema 001_init.sql:23, exposed in Create/Update API, carried in the frontend form) is never consulted. A service configured for 300 s probing is hit every 30 s, and one configured for 5 s is only probed every 30 s. Either implement per-service scheduling or remove the column/API field (note H3 makes this field even more misleading).

### M7. A new `reqwest::Client` is built for every probe, and the failure fallback strips the timeout
**File:** `crates/vexboard-server/src/probe/uptime.rs:58-62`

```rust
let client = reqwest::Client::builder().timeout(timeout)...build().unwrap_or_default();
```

- A fresh client (new connection pool, TLS config) is constructed per service per probe cycle; with N services every 30 s this is constant avoidable allocation and prevents connection reuse. Build one client at startup (the notifier already does this, main.rs:118-121).
- If `build()` ever fails, `unwrap_or_default()` substitutes `Client::default()`, which has **no timeout** — a hung endpoint would then pin probe tasks indefinitely. The fallback hides the error and removes the one protection the builder was adding.

### M8. Expired sessions are never deleted from the database
**File:** `crates/vexboard-server/src/session_store.rs:61-99`

`load()` filters out expired rows but never deletes them, and there is no periodic cleanup task (tower-sessions' `continuously_delete_expired` idiom is not used). Rows are removed only by explicit logout. Every login from every browser accumulates a row in `tower_sessions` forever — unbounded table growth and a lingering record of "valid-looking" session IDs at rest.

### M9. Duplicate-claim protection is dead code; duplicates are creatable through the UI
**Files:** `crates/vexboard-server/src/api/services.rs:501-547`, `crates/vexboard-frontend/src/components/discovery_panel.rs:109-111`

The `POST /services/{id}/claim` endpoint exists precisely to reject claiming an already-claimed unit (409), but the discovery panel posts to **`POST /api/v1/services`** directly, which has no uniqueness check. Clicking "Add" twice (or two admins racing) creates duplicate service rows for the same unit; the discovery list hides the unit only after the refresh completes. Additionally:

- `claim_service`'s own check-then-insert is racy (no UNIQUE constraint on `services.systemd_unit` in 001_init.sql:14 to back it up).
- Its `id` path parameter is admitted unused (doc comment) — the route shape is misleading.
- `create_user` (users.rs:96-107) has the same TOCTOU: the UNIQUE constraint saves correctness but the race path surfaces as a 500 "Failed to create user" instead of 409 (the setup endpoint handles this correctly — setup.rs:136-145 — the pattern just wasn't reused).

### M10. A-Z drag-and-drop reorders the wrong items when sort_orders tie
**File:** `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs:414-417` vs `:451-462`

The displayed list sorts by `sort_order` **then name** (line 416-417). The drop handler refetches and applies `remove(src_i)` / `insert(dst_i, item)` to the raw fetch order, which is `ORDER BY sort_order ASC` only (`api/services.rs:54`) — ties (the common case: every service starts at `sort_order = 0`) come back in arbitrary rowid order, not name order. The indices captured from the rendered grid are applied to a differently-ordered list, so dragging moves the **wrong service**. The section-mode handlers (lines 208-210, 350-351) re-sort with the same tiebreak before applying indices — the A-Z path is missing that one `sort_by`.

### M11. Group reordering is a no-op for UI-created groups
**File:** `crates/vexboard-frontend/src/components/modal_groups.rs:95` and `:138-167`

`do_create` always sends `"sort_order": 0`, so every group created in the modal has `sort_order = 0`. `do_move` reorders by **swapping the two groups' sort_order values** — swapping 0 with 0 changes nothing, so the up/down arrows silently do nothing until someone hand-edits sort orders. Also, the two PUTs are sequential and unguarded: if the second fails the orders are corrupted (one swap applied), with no error surfaced. Assign `max(sort_order)+1` on create, or renumber the whole list on move in one pass.

### M12. Disk I/O metrics are zero on VMs and many real systems
**File:** `crates/vexboard-server/src/metrics/system.rs:192-226`

`read_disk()` only accepts device names matching `sd?` (exactly 3 chars) or `nvme*n1` without `p`. This excludes:

- `vda`/`vdb` (virtio — most KVM/cloud VMs), `xvda` (Xen), `mmcblk0` (eMMC/SD — Raspberry Pi class hardware), `md0`/`dm-0` (RAID/LVM), and `sdaa`+ (>26 disks).
- `nvme0n2`+ namespaces (`ends_with("n1")`).

On those systems `disk_read_bytes`/`disk_write_bytes` are silently 0. Also the 512-byte sector assumption is fine for `/proc/diskstats` (it's defined in 512-byte units), but the first filter expression (lines 204-208) is redundant with `is_whole_disk` (lines 211-215) — the first check's conditions are entirely subsumed and can be deleted.

### M13. Frontend swallows every mutation error
**Files (pattern):** `crates/vexboard-frontend/src/pages/dashboard/modals.rs:33-35,53-55,101-103,131-133`, `service_grid.rs:72-75`, `quick_links_section.rs:55-58`, `modal_groups.rs:96-98,116-121,130-133,154-163`, `pages/settings.rs:257-261,281-284`, `discovery_panel.rs:109-114`

Nearly every write is `let _ = req.send().await;` followed by an unconditional `refetch()`. A 409 ("Cannot demote the last admin"), 403, or 500 produces **no feedback** — the modal closes, the list refreshes unchanged, and the user has no idea the action failed. The Add User form (settings.rs:356-371) shows the right pattern (reads the error body); the rest of the app should match it. This also hides H6/M5-class bugs from users and developers.

### M14. PAM authentication: no account validation, blocking call on the async runtime, and all-users-are-admin
**Files:** `crates/vexboard-server/src/pam_auth.rs:73-100`, `crates/vexboard-server/src/api/auth.rs:84-98,257-258`

- `authenticate_pam` calls `pam_authenticate` but never `pam_acct_mgmt`. Expired, locked, or otherwise disabled accounts (e.g. `usermod -L` only blocks via account/auth policy) can still authenticate if the password verifies. Account-validity checking is the documented second half of the PAM handshake.
- The call runs synchronously inside the async handler. `pam_unix` deliberately sleeps (~2 s) on failure (`FAIL_DELAY`), blocking a tokio worker thread per failed login — a few concurrent failed logins can stall unrelated requests. Wrap in `tokio::task::spawn_blocking`.
- Every PAM-authenticated OS user is granted `role = "admin"` (auth.rs:96, 258). Any local account on the host — including service accounts with passwords — gets full dashboard admin. At minimum this deserves a config allowlist of usernames/groups; today it's an undocumented trust grant.

### M15. Docker discovery silently cannot use TCP endpoints it appears to support
**File:** `crates/vexboard-server/src/discovery/docker.rs:53-56,78-88,98`

`socket_host()` carefully parses `tcp://` / `http://` endpoints, but the loop above it skips any socket where `Path::new(socket).exists()` is false (a TCP URL is never an existing path), and `Docker::connect_with_socket` only handles Unix sockets anyway. Configuring `sockets = ["tcp://docker-host:2375"]` is silently ignored — no log above debug level. Either support TCP via `Docker::connect_with_http` or reject non-path entries loudly at startup; the `tcp://` branch of `socket_host` is currently unreachable dead code.

### M16. systemd discovery does an O(units) DB query + procfs/subprocess scan every pass, every interval
**File:** `crates/vexboard-server/src/discovery/systemd.rs:124-139`

For each candidate unit, every 60 s pass: one `SELECT EXISTS` round-trip (fetch all claimed `systemd_unit`s once per pass instead), then `detect_url_hint` — D-Bus property reads, `/proc` scans, and possibly spawning `podman port`/`docker port` subprocesses — even for units whose hints were already computed last pass and that nobody is looking at. On a box with dozens of services this is steady-state background churn. Line 139 also logs the result at `info!` per unit per pass, flooding the journal (everything else in the detection pipeline correctly uses `debug!`).

---

## LOW

### L1. Dead `settings` table
**File:** `crates/vexboard-server/src/db/migrations/001_init.sql:45-48` — the `settings` key/value table is created and referenced nowhere in the codebase.

### L2. Unused/misplaced dependencies
**File:** `crates/vexboard-server/Cargo.toml:32-33` — `thiserror` is unused (no references in the crate). `tower` is used only by `tests.rs` (`ServiceExt::oneshot`) and belongs in `[dev-dependencies]`.

### L3. Audit log pagination is unstable on timestamp ties
**File:** `crates/vexboard-server/src/api/audit.rs:67-68` — `ORDER BY created_at DESC` with second-resolution `DATETIME` means same-second entries have nondeterministic relative order; rows can repeat or vanish across pages. Add `, id DESC`.

### L4. Rate limiter counts successful logins against the budget
**File:** `crates/vexboard-server/src/api/auth.rs:70`, `rate_limit.rs:25-38` — `check()` records an attempt before the credential check, and successes are never refunded. Ten users behind one NAT logging in legitimately within a minute lock out the eleventh. Common practice is to count only failures (or reset on success).

### L5. `parse_port_from_listen_address` misparses bare IPv6 addresses
**File:** `crates/vexboard-server/src/discovery/systemd.rs:657-672` — `"::1"` splits on the last `:` and yields port **1**; any `Listen` value of a bare IPv6 host produces a bogus low port hint.

### L6. Network metrics count virtual interfaces and present lifetime totals as "IN/OUT"
**Files:** `crates/vexboard-server/src/metrics/system.rs:167-189`, `crates/vexboard-frontend/src/components/metric_bar.rs:116-139` — `read_network()` sums every non-`lo` interface, double-counting bridge/veth traffic on Docker hosts (each packet crosses eth0 *and* docker0/veth). The metric bar then renders the cumulative since-boot counter ("IN 1.4 TB") in a position that reads like a live rate. If a rate is intended, diff successive snapshots client- or server-side.

### L7. CPU sampling: comment says 1 s, code sleeps 250 ms; snapshot endpoint inherits the latency
**File:** `crates/vexboard-server/src/metrics/system.rs:96-101` — doc/code mismatch, and every call to `/api/v1/metrics/snapshot` blocks ~250 ms by design. Maintaining the previous sample in state would make snapshots instant and the comment true.

### L8. Edit modal's favicon auto-fill can clobber a manually chosen icon
**File:** `crates/vexboard-frontend/src/components/modal_edit.rs:62,95-105` — `icon_auto` starts `true` even when editing a service that has a hand-set icon; the first keystroke in the URL field replaces the custom icon with `<host>/favicon.ico`. Initialize `icon_auto` to `initial.icon.is_empty()`. Same pattern in `quick_link_modal.rs:46`.

### L9. "Add Service" modal retains stale form state across opens
**Files:** `crates/vexboard-frontend/src/pages/dashboard/modals.rs:70-77`, `modal_edit.rs:56-62` — the Add modal is mounted once and toggled via `Show`; its signals are initialized once, so reopening after a save shows the previous service's values. (The discovery panel works around this by remounting — discovery_panel.rs:124-125; the dashboard Add path doesn't.)

### L10. Group rename fires twice on Enter
**File:** `crates/vexboard-frontend/src/components/modal_groups.rs:292-299` — Enter triggers `do_rename(id)`, which closes the editor, which blurs the input, whose `on:blur` calls `do_rename(id)` again → two PUTs and a double `on_saved` refetch. Harmless today (idempotent rename) but a latent double-submit.

### L11. EventSource and listener are leaked by design
**File:** `crates/vexboard-frontend/src/components/metric_bar.rs:71-86` — the SSE `EventSource` is never closed and the closure is `.forget()`-ed. With `MetricBar` mounted once per page load this is bounded, but any future remount of `MainLayout` stacks live connections. Use Leptos' `on_cleanup` to `es.close()`.

### L12. `Request::json(...).unwrap()` in WASM handlers
**Files:** `crates/vexboard-frontend/src/pages/login.rs:24`, `pages/setup.rs:31`, `components/user_menu.rs:125` — serialization of these payloads can't realistically fail, but a panic here aborts the WASM app with a blank screen; everywhere else the code uses `if let Ok(req)`. Inconsistent for no benefit.

### L13. `server_services_only` doc comment describes behavior the code doesn't have
**File:** `crates/vexboard-server/src/config.rs:68-72` vs `discovery/systemd.rs:111-117` — the config comment says it filters by unit-file location (`/etc/systemd/system/`), but the implementation filters by `sub_state == "running"`. `config/default.toml:29-30` documents it correctly; the rustdoc is wrong.

### L14. `assets_path = "embedded"` is a misnomer and CWD-dependent
**File:** `crates/vexboard-server/src/main.rs:134-143` — "embedded" actually means "serve from `./assets` relative to the process CWD"; nothing is embedded in the binary (CLAUDE.md's architecture note says assets are embedded). Outside the Nix module (which sets an absolute path), starting the binary from a different directory silently serves 404s/blank pages. Same CWD fragility applies to `config/default` in `config.rs:148`.

### L15. Username inputs are trim-validated but stored raw
**Files:** `crates/vexboard-server/src/api/users.rs:77`, `api/auth.rs:369-376`, `api/setup.rs:95` — emptiness is checked via `trim().is_empty()`, but the untrimmed value is inserted, so `" bob"` and `"bob"` are distinct users that render identically. Trim before storing/binding.

### L16. bcrypt silently truncates passwords at 72 bytes
**Files:** `crates/vexboard-server/src/api/setup.rs:107`, `api/users.rs:109`, `api/auth.rs:417` — standard bcrypt behavior, but there's no max-length validation, so characters beyond 72 bytes are silently ignored at both set and verify time. Worth an explicit length cap with a clear error.

### L17. No confirmation on destructive actions
**Files:** `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs:70-77`, `quick_links_section.rs:53-60`, `modal_groups.rs:127-136`, `pages/settings.rs:279-291` — one accidental click permanently deletes a service, quick link, group, or **user account**, with errors also suppressed (M13). At minimum gate user deletion behind a confirm.

### L18. `trigger_refresh` has no debounce or concurrency guard
**File:** `crates/vexboard-server/src/discovery/mod.rs:74-95` — every request spawns fresh systemd + docker scans (D-Bus enumeration, subprocess spawning per M16). Hammering the endpoint (admin-only, but the frontend also fires it after every claim — discovery_panel.rs:112) stacks unbounded concurrent scans whose interleaved `retain`+`extend` writes can also momentarily duplicate or drop list entries between two concurrent passes.

### L19. Notification loop's `prev_status` map never prunes deleted services
**File:** `crates/vexboard-server/src/notify.rs:26,94` — entries persist for service IDs that no longer exist. Bounded by historical service count; trivial, but a `retain` against current IDs on occasion would tidy it.

### L20. Discovery interval of `0` busy-loops
**Files:** `crates/vexboard-server/src/discovery/systemd.rs:70-78`, `discovery/docker.rs:23-31`, `metrics/system.rs:79-93`, `probe/mod.rs:19` — none of the loop intervals validate non-zero; a misconfigured `interval_secs = 0` (or `push_interval_ms = 0`) spins the loop at full speed (the systemd one re-scanning D-Bus continuously). Cheap startup validation in `AppConfig::load()` would prevent it.

### L21. Probe-status fetch failure is indistinguishable from "no data"
**File:** `crates/vexboard-server/src/api/services.rs:71-82` — the latest-probe query error is swallowed by `unwrap_or_default()`, so a DB error renders every service as "unknown" with no log line (contrast with the services query above it, which logs and 500s).

### L22. Swagger UI and OpenAPI spec are publicly exposed
**File:** `crates/vexboard-server/src/api/mod.rs:45-47` — `/swagger-ui` and `/api-docs/openapi.json` are mounted outside the auth layers, enumerating the full API surface (including admin routes) to unauthenticated visitors. For a LAN dashboard this is arguably fine, but it's an information-disclosure default worth gating or documenting.

---

## Cross-cutting observations (no single fix location)

1. **Audit `actor` falls back to `"unknown"`** in every handler (`session.get(...).unwrap_or("unknown")`). Since all these routes sit behind `require_auth`/`require_admin`, a missing username indicates a broken session and should probably be an error, not a silently mislabeled audit row.
2. **The check-then-act pattern** (read row → validate → write) used across users/groups/services handlers is not transactional. Single-writer SQLite makes the practical risk low, but the last-admin guard (M4) and username-uniqueness checks are the two places where a race has security consequences; both deserve a transaction or a constraint-backed error path.
3. **Multi-step updates aren't atomic**: `update_me` applies username and password in separate statements (`api/auth.rs:386-439`) — a failure between them leaves a half-applied credential change (audited as one event).

## Summary counts

| Priority | Count |
|----------|-------|
| HIGH     | 6     |
| MEDIUM   | 16    |
| LOW      | 22    |

The highest-leverage fixes, in order: stop trusting `X-Forwarded-For` (H1), invalidate sessions on role change/deletion and wire up the session TTL/secret (H2+H4), fix the edit-modal probe reset (H3), store container claims without a fake `systemd_unit` (H5), and fix the role `<select>` cast (H6).
