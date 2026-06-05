# VexBoard — Project Audit Report
**Date:** 2026-06-04
**Scope:** Full codebase — read-only analysis, no changes made
**Auditor:** Claude Code

---

## PART 1: PROJECT HEALTH AUDIT

### 1.1 Structure & Architecture

**Architectural pattern:** The project follows a clean Cargo workspace monorepo pattern with two crates:
- `crates/vexboard-server` — native Axum server binary
- `crates/vexboard-frontend` — WASM-only Leptos CSR app

The separation is intentional and well-enforced. The backend uses a standard Axum router with `AppState` extraction, broadcast channels for real-time SSE, and spawned Tokio tasks for background work (discovery, probing, metrics). The frontend follows Leptos conventions with `#[component]` macros, reactive signals, and client-side routing.

**Consistent:** Yes. Both crates follow their respective framework idioms. Workspace-level dependency management via `[workspace.dependencies]` prevents version drift.

**Misplaced / misnamed / orphaned files:** None detected. All files serve a documented purpose.

**Circular dependencies / import cycles:** None. Frontend and server are separate crates with no shared code crate; they communicate only via HTTP/SSE at runtime.

**Dead code:**

| Location | Type | Severity | Notes |
|---|---|---|---|
| `crates/vexboard-server/src/db/models.rs:50–54` | Struct `Setting` defined but never instantiated or used anywhere | 🟡 Medium | Suggests a planned feature (persistent settings store) that was never implemented |
| `crates/vexboard-frontend/src/components/modal_edit.rs:26` | `#[allow(dead_code)] pub group_id: Option<i64>` — field declared but never used in form submission | 🔵 Low | Placeholder for group assignment UI |
| `crates/vexboard-frontend/src/components/discovery_panel.rs:10,12` | `active_state` and `sub_state` fields in `DiscoveredUnitFe` — deserialized but never rendered in UI | 🔵 Low | Planned display feature |
| `crates/vexboard-frontend/src/pages/discovered.rs` | 6-line stub page, no real content | 🔵 Low | Placeholder page route |

**Files too large:** No source file exceeds 500 lines. `pages/dashboard.rs` at 459 lines is the largest and is a reasonable candidate for future extraction (multiple modals, service grid, real-time state) but not an urgent concern.

---

### 1.2 Correctness & Bugs

**Logic errors / incorrect conditionals:**

- `crates/vexboard-server/src/api/services.rs:98–102` and `crates/vexboard-server/src/discovery/docker.rs:117–123`: Both use `SELECT COUNT(*)` for duplicate detection before insert. This is a classic TOCTOU (time-of-check/time-of-use) race — two concurrent requests can both pass the check and both insert. The database `UNIQUE` constraint on `systemd_unit` catches this at the DB level, but the error is returned as `500 InternalServerError` rather than `409 Conflict`, which is misleading to callers.

- `crates/vexboard-server/src/api/services.rs:33–54`: **N+1 query** in `list_services`. For every service returned by `SELECT * FROM services`, a second query `SELECT ... FROM probe_results WHERE service_id = ?` is issued in a loop. With 100 services this becomes 101 database round-trips. Should be replaced with a `LEFT JOIN` or a window function CTE.

- `crates/vexboard-server/src/api/services.rs:73–74`: Tags serialization failure silently falls back to an empty string via `.unwrap_or_default()`, causing silent data loss.

**Unhandled error paths / silent failures:**

- `crates/vexboard-server/src/probe/uptime.rs:67–73`: Both `sqlx::query().execute()` calls and the broadcast `tx.send()` use `let _ = ...` — errors are completely swallowed with no logging. Probe results may fail to persist without any diagnostic trail.

- `crates/vexboard-server/src/api/auth.rs:92`: `session.insert("username", ...).await.ok()` — session write failure is silently ignored. A user would be told they are logged in but the session may not persist.

- `crates/vexboard-server/src/api/setup.rs:38–42`: The `unwrap_or(1)` fallback on the user count check masks database errors, potentially allowing setup to fail silently.

**Race conditions:**

- `crates/vexboard-server/src/api/setup.rs`: Counts existing users then inserts. Two simultaneous first-run requests could both observe zero users and both attempt to create an admin account. The `UNIQUE` constraint on `username` catches the second insert, but the error handling path does not distinguish this from other DB errors.

**Hardcoded values:**

- `crates/vexboard-server/src/discovery/docker.rs:135`: `format!("http://localhost:{port}")` — hardcodes `localhost` for Docker-discovered container URLs. This is incorrect for remote Docker hosts or cluster deployments.

- `config/default.toml:12`: `secret = "change-me-in-production"` — placeholder auth secret. Overridable via `VEXBOARD_AUTH_SECRET` env var and clearly marked, but represents a risk if overlooked.

**TODO / FIXME comments:**

| Location | Comment | Severity |
|---|---|---|
| `crates/vexboard-server/src/main.rs:102` | `// TODO: use a persistent SQLite-backed store for production deployments` | 🟡 Medium |

---

### 1.3 Consistency

**Naming conventions:** Excellent. `snake_case` for functions, variables, and file names; `PascalCase` for types and structs; consistent `mod.rs` for module boundaries. No violations found.

**Formatting and style:** Consistent. `cargo fmt` is enforced in CI and in `scripts/preflight.sh`. `-D warnings` on clippy ensures no silent lint issues.

**Similar problems solved differently:**

1. Auth query duplication — `crates/vexboard-server/src/api/auth.rs:62` and `:165` both run the identical `SELECT id, username, password_hash, created_at FROM users WHERE username = ?` query directly, with no shared helper function.

2. Feature-gated auth functions — `login()`, `me()`, and `update_me()` are each defined twice with `#[cfg(feature = "pam-auth")]` guards. This is a legitimate pattern but results in ~150 lines of near-duplicate code.

**Error message format:** Consistent. All backend errors use `tracing::error!("operation: {e}")` pattern. No `println!` or `dbg!` debug prints found anywhere in source.

---

### 1.4 Configuration & Environment

**Missing / redundant / conflicting entries:** None. All seven config sections (`server`, `database`, `auth`, `discovery`, `docker`, `probe`, `metrics`) are present and internally consistent.

**Secrets risk:**
- `config/default.toml:12`: `secret = "change-me-in-production"` — committed placeholder. README documents the env var override, but operators who don't read the docs may deploy with this value.
- `docker-compose.yml`: `VEXBOARD_AUTH_SECRET: "change-me"` — a second committed placeholder secret.

**Environment separation:** Good. The config load order (env vars > `/etc/vexboard/config.toml` > `config/default.toml`) correctly separates deployment values from defaults.

**Dependency versions:** All workspace dependencies use flexible SemVer specs (e.g., `"1"`, `"0.8"`) pinned via `Cargo.lock` — best practice for applications. Cargo.lock v4 format with 380+ dependencies fully pinned.

**Deprecated dependencies:**
- `paste` crate (RUSTSEC-2024-0436) — unmaintained, transitive via Leptos. Documented in `audit.toml`. No fix available pending upstream Leptos update.
- `rsa` crate (RUSTSEC-2023-0071) — Marvin Attack, transitive via `sqlx-mysql` optional dep. Documented in `audit.toml`. Not in the binary dependency graph (SQLite-only workspace). Correctly assessed.

---

### 1.5 Documentation

**Undocumented public APIs:** No OpenAPI/Swagger spec exists. The REST API (services, groups, quick-links, auth, metrics SSE, discovery, health) is entirely undocumented in machine-readable form.

**Inaccurate documentation:** None found — README accurately describes the architecture, deployment options, and configuration.

**README completeness:** Present, complete, and current (`README.md`). Covers: features, Docker Compose quickstart, NixOS module, dev shell, architecture diagram, configuration reference, tech stack, license.

**Setup / build / deployment instructions:** Present and correct for all three deployment targets (Docker, NixOS, development via Nix flake).

**Gaps:**
- No troubleshooting guide for common issues
- No API endpoint reference (no OpenAPI / Swagger)
- No architecture deep-dive document

---

### 1.6 Tests

**Apparent coverage:** Critically low. Only **2 unit tests** exist in the entire codebase:

- `crates/vexboard-server/src/discovery/systemd.rs:150` — `test_exclusion_exact()`: tests exact pattern matching in service exclusion list
- `crates/vexboard-server/src/discovery/systemd.rs:157` — `test_exclusion_glob()`: tests glob (`*`) pattern matching in service exclusion list

**Untested critical paths (all of them):**
- All API handlers: auth, services, groups, quick-links, discovery, metrics
- Database layer: models, migrations, queries
- Probe logic: HTTP client, timeout, result storage, history pruning
- Metrics collection: `/proc` filesystem parsing
- Docker discovery: bollard client, container enumeration
- Session management: login, logout, session validation
- PAM authentication flow
- Configuration loading and env var override

**Skipped / ignored tests:** None found (because there are almost no tests).

**Test quality:** The 2 existing tests are well-named and test meaningful behavior.

**Missing edge cases:** Effectively all edge cases are untested given the near-zero coverage.

---

### 1.7 Security

**Missing authentication on API endpoints — CRITICAL:**

There is **no session/authentication middleware applied to any non-auth endpoint**. Client-side Leptos code is the only enforcement mechanism. Any unauthenticated HTTP client can directly call:

- Read, create, update, delete services — `crates/vexboard-server/src/api/services.rs`
- Read, create, update, delete groups — `crates/vexboard-server/src/api/groups.rs`
- Read, create, update, delete quick links — `crates/vexboard-server/src/api/quick_links.rs`
- Trigger service discovery — discovery endpoints
- Stream real-time system metrics via SSE — `crates/vexboard-server/src/api/metrics.rs`

Only `/auth/login`, `/auth/logout`, `/auth/me`, `/setup`, and `/health` are in the auth module. All other endpoints have no server-side session check.

**Session cookie `with_secure(false)` — HIGH:**

`crates/vexboard-server/src/main.rs:103`:
```rust
let session_layer = SessionManagerLayer::new(session_store).with_secure(false);
```
`.with_secure(false)` disables the `Secure` cookie flag, allowing session cookies to transmit over plain HTTP. On any unencrypted local network connection, sessions are vulnerable to interception.

**CORS allow-all — MEDIUM-HIGH:**

`crates/vexboard-server/src/main.rs:120–123`:
```rust
CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any)
```
Permissive CORS is documented as "development-friendly" but there is no environment-conditional configuration to restrict it in production.

**Input validation:** No injection risks detected. All database queries use SQLx parameterized bindings. No path traversal vectors found.

**Password storage:** Bcrypt used correctly for password hashing (`crates/vexboard-server/src/api/auth.rs`). No plaintext passwords stored.

**PAM auth:** `crates/vexboard-server/src/pam_auth.rs` — multiple `unsafe` blocks for C FFI, all correctly documented with safety comments, null-check PAM return values, and proper C memory cleanup. Assessed as acceptable.

**Dependency vulnerabilities:** 2 known, both documented in `audit.toml` with correct impact assessments. No unmitigated vulnerabilities found.

---

### 1.8 Performance

**N+1 queries — HIGH:**

`crates/vexboard-server/src/api/services.rs:22–54`: The `list_services` handler issues one query for all services, then one per-service query for its latest probe result. 100 services → 101 database round-trips. With SQLite this is local I/O, so latency per query is low, but growth is linear with service count.

**Inefficient existence checks — LOW:**

`SELECT COUNT(*) FROM ...` is used in three places where `EXISTS` would short-circuit on first match:
- `crates/vexboard-server/src/discovery/systemd.rs:98–102`
- `crates/vexboard-server/src/discovery/docker.rs:117–123`
- `crates/vexboard-server/src/api/services.rs:234–238`

**Metrics streaming — GOOD:** SSE implementation using tokio broadcast channels is efficient (`crates/vexboard-server/src/api/metrics.rs`). No N+1 or polling on the metrics path.

**Probe history pruning — ADEQUATE:** `crates/vexboard-server/src/probe/uptime.rs:76–84` uses `DELETE WHERE id NOT IN (SELECT id ... ORDER BY checked_at DESC LIMIT ?)` — correct but a window function approach would be more efficient at scale.

**Caching:** No expensive operations identified that should but don't have caching. Discovery results are held in `Arc<RwLock<Vec<...>>>` in memory, which is appropriate.

**Memory leaks:** None identified. Broadcast channels are bounded by receiver count; `Arc`/`RwLock` usage is correct.

---

### PART 1 SUMMARY

**Issue Severity Table**

| Severity | Count | Examples |
|---|---|---|
| 🔴 Critical | 1 | No auth middleware on any non-auth API endpoint |
| 🟠 High | 3 | Session cookie `with_secure(false)`; N+1 query in `list_services`; only 2 tests for ~4,500 lines of code |
| 🟡 Medium | 5 | Allow-all CORS; in-memory session store (documented TODO); silent probe DB write failure; silent session insert failure; setup endpoint race condition |
| 🔵 Low | 7 | Hardcoded `localhost` in Docker discovery; unused `Setting` struct; dead `group_id` field; `discovered.rs` stub; tags serialization silent data loss; duplicate auth query; `COUNT` vs `EXISTS` |
| ⚪ Info | 4 | No API docs (OpenAPI); no troubleshooting guide; feature-gated auth duplication verbose but functional; placeholder secrets in committed files |

**Overall Health Score: 6.5 / 10**

VexBoard is a well-architected, actively developed project with solid fundamentals: clean crate separation, consistent naming, good CI/CD, comprehensive configuration management, and proper dependency pinning. The primary weaknesses are a critical authentication gap (client-side Leptos provides security the backend doesn't enforce), severely insufficient test coverage (2 tests for ~4,500 lines of code), and a production-unfriendly session cookie configuration. These are the kind of issues common in early-stage projects where features are prioritized before hardening — all are fixable without architectural changes. The score reflects genuine production-readiness concerns rather than any structural flaw.

---

## PART 2: IDEAS & RECOMMENDATIONS

### 2.1 Quick Wins (low effort, high value)

**1. Add session authentication middleware to the Axum router**
- **What:** Create an `auth_required` middleware (using `tower::layer_fn` or a custom layer) that checks for a valid session and returns `401 Unauthorized` if absent. Apply it to all routes except `/api/v1/setup`, `/api/v1/auth/login`, `/api/v1/auth/logout`, and `/health`.
- **Why:** Single highest-impact fix — closes the critical security hole where the entire API is publicly accessible without authentication.
- **Files:** `crates/vexboard-server/src/api/mod.rs`, `crates/vexboard-server/src/main.rs`
- **Effort:** ~1–2 hours

**2. Fix N+1 query in `list_services`**
- **What:** Replace the per-service probe result query loop with a single `LEFT JOIN` or `WITH latest_probes AS (...)` CTE that fetches all latest probe results in one query.
- **Why:** Linear database query growth will noticeably degrade dashboard load time as service count grows.
- **Files:** `crates/vexboard-server/src/api/services.rs:22–54`
- **Effort:** 30–45 minutes

**3. Add error logging to silently-failing probe DB writes**
- **What:** Replace `let _ = sqlx::query(...).execute(db).await` with `if let Err(e) = ... { tracing::error!(...) }` in `uptime.rs` and do the same for the session insert in `auth.rs`.
- **Why:** Silent failures are invisible in production; operators cannot diagnose missed probe results without log entries.
- **Files:** `crates/vexboard-server/src/probe/uptime.rs:67–73`, `crates/vexboard-server/src/api/auth.rs:92`
- **Effort:** 15 minutes

**4. Make session `with_secure` configurable**
- **What:** Add `[auth] secure_cookies = true` to `config/default.toml`, parse it in `config.rs`, and wire it to `.with_secure(cfg.auth.secure_cookies)` at startup. This lets HTTP-only self-hosted deployments opt out explicitly.
- **Why:** As-is, session cookies transmit in cleartext over HTTP, enabling session hijacking on local networks.
- **Files:** `crates/vexboard-server/src/main.rs:103`, `config/default.toml`, `crates/vexboard-server/src/config.rs`
- **Effort:** 30 minutes

**5. Remove or implement the `Setting` struct**
- **What:** Either delete `crates/vexboard-server/src/db/models.rs:50–54` (the `Setting` struct and its `#[allow(dead_code)]`) or implement the persistent settings store it implies.
- **Why:** Dead code signals an abandoned feature and adds confusion for future contributors.
- **Files:** `crates/vexboard-server/src/db/models.rs`
- **Effort:** 10 minutes (delete) or 2–4 hours (implement)

---

### 2.2 Feature Recommendations

**1. Persistent SQLite-backed session store**
- **Problem:** The current `MemoryStore` loses all sessions on server restart, forcing all users to re-login after any update or crash. Already flagged as a TODO at `main.rs:102`.
- **Value:** Dramatically improves operator experience in production.
- **Complexity:** Low — `tower-sessions-sqlx-store` or a custom SQLite table implementation.
- **Builds on:** Existing `sqlx` pool in `AppState`, existing session layer setup in `main.rs:101–105`.

**2. CORS origin allowlist via configuration**
- **Problem:** Allow-all CORS is a security liability for production deployments behind a reverse proxy with a known origin.
- **Value:** Closes a medium-high security gap without breaking legitimate use cases.
- **Complexity:** Low — add `[server] allowed_origins = ["*"]` to `config/default.toml`, parse in `config.rs`, wire to `CorsLayer`.
- **Builds on:** `crates/vexboard-server/src/config.rs`, `crates/vexboard-server/src/main.rs:119–124`.

**3. API rate limiting on the login endpoint**
- **Problem:** No per-IP request rate limiting. The login endpoint is vulnerable to brute-force attacks.
- **Value:** Basic protection against credential stuffing.
- **Complexity:** Medium — `tower-governor` crate provides ready-made rate limiting as an Axum layer.
- **Builds on:** Existing Axum middleware stack in `main.rs`.

**4. Audit log for sensitive operations**
- **Problem:** No record of who created/deleted services, changed credentials, or triggered discovery. Compliance and incident investigation concern for shared deployments.
- **Value:** Accountability and diagnostic capability.
- **Complexity:** Medium — add `audit_log` table to migration, create `AuditEvent` model, insert records in state-mutating handlers.
- **Builds on:** Existing `db/` layer (`sqlx`, `models.rs`), all CRUD handlers in `api/`.

**5. OpenAPI / Swagger API documentation**
- **Problem:** REST API has no machine-readable specification. External integration requires reading source code.
- **Value:** Enables third-party integrations, simplifies testing, provides a canonical API contract.
- **Complexity:** Medium — `utoipa` crate generates OpenAPI 3.x specs from handler annotations; `utoipa-swagger-ui` serves the browser UI.
- **Builds on:** All files in `crates/vexboard-server/src/api/`.

**6. Service grouping in the UI**
- **Problem:** The `groups` table and full CRUD API exist; `group_id` is a field on services; but `EditFormData.group_id` in the frontend is dead code. Users cannot assign services to groups via the UI.
- **Value:** Completes a partially-built feature that enables logical organization of large service lists.
- **Complexity:** Low-Medium — backend is fully implemented; only the frontend modal and dashboard rendering need updating.
- **Builds on:** `crates/vexboard-frontend/src/components/modal_edit.rs:26`, `crates/vexboard-server/src/api/groups.rs`, existing `groups` DB table.

**7. Webhook / notification support for probe state changes**
- **Problem:** When a service goes down, there is no mechanism to alert an operator outside the dashboard.
- **Value:** Makes VexBoard useful for on-call workflows, not just passive monitoring.
- **Complexity:** Medium — add `[notifications]` config section, implement a webhook sender triggered from the probe broadcast receiver.
- **Builds on:** `crates/vexboard-server/src/probe/`, `reqwest` (already a dependency), `config.rs`.

**8. Dashboard drag-to-reorder services**
- **Problem:** `sort_order` columns exist on `services` and `groups` tables but there is no drag-and-drop UI on the dashboard.
- **Value:** Basic UX improvement enabling users to arrange their dashboard layout.
- **Complexity:** Medium — requires frontend drag-and-drop (via `web-sys` `DragEvent`) and a `PATCH /api/v1/services/reorder` endpoint.
- **Builds on:** `sort_order` columns already in schema (`db/migrations/001_init.sql`), `crates/vexboard-server/src/api/services.rs`.

**9. Multi-user access control (roles)**
- **Problem:** All authenticated users have identical permissions. No read-only viewer role exists.
- **Value:** Enables sharing the dashboard with team members without full admin privileges.
- **Complexity:** High — add `role` column to `users`, check role in each handler, surface role management in settings.
- **Builds on:** `crates/vexboard-server/src/api/auth.rs`, `crates/vexboard-server/src/db/models.rs`, `crates/vexboard-frontend/src/pages/settings.rs`.

**10. Dark/light mode toggle**
- **Problem:** UI is hard-coded to dark mode (`<html class="dark">` in `crates/vexboard-frontend/src/main.rs:82`). There is no toggle.
- **Value:** Accessibility and user preference support.
- **Complexity:** Low — add a theme signal, persist to `localStorage`, toggle `dark` class on `<html>`, add a button to sidebar or user menu.
- **Builds on:** `crates/vexboard-frontend/src/main.rs`, `crates/vexboard-frontend/src/components/sidebar.rs` or `user_menu.rs`.

---

### 2.3 Refactoring Opportunities

**1. Extract authentication middleware into its own module**
- **What:** Create `crates/vexboard-server/src/middleware/auth.rs` with a reusable session-check layer, applied at the router level.
- **Why:** Centralizes the authorization boundary — future endpoints are protected by default, and policy changes require edits in one place instead of every handler.
- **Risk:** Low — purely additive; existing handler signatures are unaffected.

**2. Consolidate feature-gated auth handler duplication**
- **What:** Replace the four pairs of `#[cfg(feature = "pam-auth")]` / `#[cfg(not(feature = "pam-auth"))]` function definitions in `crates/vexboard-server/src/api/auth.rs` with a single handler calling into an internal abstraction.
- **Why:** ~150 lines of near-duplicate code; any change to auth behavior (2FA, rate limiting, audit logging) must be applied in both branches.
- **Risk:** Low-Medium — logic is verified by existing CI.

**3. Split `dashboard.rs` into sub-components**
- **What:** Extract from `crates/vexboard-frontend/src/pages/dashboard.rs` (459 lines):
  - `ServiceGrid` component (service card rendering + sort state)
  - `DashboardModals` component (edit modal, quick link modal, discovery panel)
  - Optionally `QuickLinksSection`
- **Why:** The file mixes three distinct concerns and is the largest in the codebase. It will grow as features are added.
- **Risk:** Low — Leptos component extraction is well-defined; signals can be passed as props or via context.

**4. Create a shared user query helper in the `db/` module**
- **What:** Extract the repeated `SELECT id, username, password_hash, created_at FROM users WHERE username = ?` query at `crates/vexboard-server/src/api/auth.rs:62` and `:165` into a `db::get_user_by_username(pool, username)` helper.
- **Why:** DRY principle — if the query or model changes, one update instead of two.
- **Risk:** Minimal — mechanical extraction.

**5. Replace `COUNT(*)` with `EXISTS` for duplicate detection**
- **What:** Change the three `SELECT COUNT(*) FROM ... WHERE ...` existence checks to `SELECT EXISTS(SELECT 1 FROM ... WHERE ... LIMIT 1)` in:
  - `crates/vexboard-server/src/discovery/systemd.rs:98–102`
  - `crates/vexboard-server/src/discovery/docker.rs:117–123`
  - `crates/vexboard-server/src/api/services.rs:234–238`
- **Why:** `EXISTS` short-circuits on first match; `COUNT(*)` scans all matching rows. Semantically equivalent, more correct by intent.
- **Risk:** None.

---

### 2.4 Tooling & Workflow Suggestions

**1. Add a git pre-commit hook for formatting and lint**
- Currently `cargo fmt --check` and `cargo clippy` are only enforced in CI and via manual `scripts/preflight.sh`. A git pre-commit hook catches violations before they reach CI, shortening the feedback loop.
- **Implementation:** `.git/hooks/pre-commit` script (or `cargo-husky` / `lefthook`) running `cargo fmt --all -- --check` on staged `.rs` files.

**2. Build out the test suite — start with API handler integration tests**
- With 2 tests in the entire codebase, `cargo test --workspace` passes trivially and provides no regression safety. Adding skeleton integration tests using `axum::http::Request` + `tower::ServiceExt` would give CI a meaningful gate.
- **Start with:** Auth endpoint tests (login success, login failure, unauthenticated 401), then services CRUD.

**3. Add `SQLX_OFFLINE=true` support to the dev shell**
- `flake.nix` sets `DATABASE_URL="sqlite:./dev.db"`, which requires the database file to exist for SQLx compile-time query checking. Supporting `SQLX_OFFLINE=true` with a committed `sqlx-data.json` would allow offline builds and clean CI caches.
- **Files:** `flake.nix`, `.github/workflows/ci.yml`

**4. Add a `docker-compose.override.yml` for local development**
- The current `docker-compose.yml` targets production (uses the published `ghcr.io` image). A `docker-compose.override.yml` with bind-mounted source, local build, and `RUST_LOG=debug` would let developers test the full stack locally without pushing an image.

**5. Add OpenAPI spec generation as a CI artifact**
- Once `utoipa` is added (see Feature Recommendation #5), add a CI step that generates the OpenAPI spec and uploads it as a build artifact, giving operators a versioned API reference without running the server.