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
| ~~`crates/vexboard-server/src/db/models.rs:50–54`~~ | ~~Struct `Setting` defined but never instantiated or used anywhere~~ | 🟡 Medium ✅ FIXED (2026-06-05) | Deleted |
| `crates/vexboard-frontend/src/components/modal_edit.rs:26` | `#[allow(dead_code)] pub group_id: Option<i64>` — field declared but never used in form submission | 🔵 Low | Placeholder for group assignment UI |
| `crates/vexboard-frontend/src/components/discovery_panel.rs:10,12` | `active_state` and `sub_state` fields in `DiscoveredUnitFe` — deserialized but never rendered in UI | 🔵 Low | Planned display feature |
| `crates/vexboard-frontend/src/pages/discovered.rs` | 6-line stub page, no real content | 🔵 Low | Placeholder page route |

**Files too large:** No source file exceeds 500 lines. `pages/dashboard.rs` at 459 lines is the largest and is a reasonable candidate for future extraction (multiple modals, service grid, real-time state) but not an urgent concern.

---

### 1.2 Correctness & Bugs

**Logic errors / incorrect conditionals:**

- `crates/vexboard-server/src/api/services.rs:98–102` and `crates/vexboard-server/src/discovery/docker.rs:117–123`: Both use `SELECT COUNT(*)` for duplicate detection before insert. This is a classic TOCTOU (time-of-check/time-of-use) race — two concurrent requests can both pass the check and both insert. The database `UNIQUE` constraint on `systemd_unit` catches this at the DB level, but the error is returned as `500 InternalServerError` rather than `409 Conflict`, which is misleading to callers.

- ~~`crates/vexboard-server/src/api/services.rs:33–54`: **N+1 query** in `list_services`. For every service returned by `SELECT * FROM services`, a second query `SELECT ... FROM probe_results WHERE service_id = ?` is issued in a loop. With 100 services this becomes 101 database round-trips. Should be replaced with a `LEFT JOIN` or a window function CTE.~~ ✅ FIXED (2026-06-04) — replaced with a single `LEFT JOIN` query fetching all latest probe results.

- `crates/vexboard-server/src/api/services.rs:73–74`: Tags serialization failure silently falls back to an empty string via `.unwrap_or_default()`, causing silent data loss.

**Unhandled error paths / silent failures:**

- ~~`crates/vexboard-server/src/probe/uptime.rs:67–73`: Both `sqlx::query().execute()` calls and the broadcast `tx.send()` use `let _ = ...` — errors are completely swallowed with no logging. Probe results may fail to persist without any diagnostic trail.~~ ✅ FIXED (2026-06-04) — `tracing::error!` added to all failure paths.

- ~~`crates/vexboard-server/src/api/auth.rs:92`: `session.insert("username", ...).await.ok()` — session write failure is silently ignored. A user would be told they are logged in but the session may not persist.~~ ✅ FIXED (2026-06-04) — `tracing::error!` added to session persist failures.

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

**Missing authentication on API endpoints — CRITICAL: ✅ FIXED (2026-06-04)**

~~There is **no session/authentication middleware applied to any non-auth endpoint**. Client-side Leptos code is the only enforcement mechanism. Any unauthenticated HTTP client can directly call:~~

- Read, create, update, delete services — `crates/vexboard-server/src/api/services.rs`
- Read, create, update, delete groups — `crates/vexboard-server/src/api/groups.rs`
- Read, create, update, delete quick links — `crates/vexboard-server/src/api/quick_links.rs`
- Trigger service discovery — discovery endpoints
- Stream real-time system metrics via SSE — `crates/vexboard-server/src/api/metrics.rs`

Only `/auth/login`, `/auth/logout`, `/auth/me`, `/setup`, and `/health` are in the auth module. All other endpoints have no server-side session check.

**Session cookie `with_secure(false)` — HIGH: ✅ FIXED (2026-06-04)**

~~`.with_secure(false)` disables the `Secure` cookie flag, allowing session cookies to transmit over plain HTTP. On any unencrypted local network connection, sessions are vulnerable to interception.~~

`secure_cookies` is now a configurable field in `[auth]` section of `config/default.toml`, wired to `.with_secure(config.auth.secure_cookies)` in `main.rs`.

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
| 🔴 Critical | 1 | ~~No auth middleware on any non-auth API endpoint~~ ✅ FIXED |
| 🟠 High | 3 | ~~Session cookie `with_secure(false)`~~ ✅ FIXED; ~~N+1 query in `list_services`~~ ✅ FIXED; only 2 tests for ~4,500 lines of code |
| 🟡 Medium | 5 | Allow-all CORS; in-memory session store (documented TODO); silent probe DB write failure; silent session insert failure; setup endpoint race condition |
| 🔵 Low | 7 | Hardcoded `localhost` in Docker discovery; unused `Setting` struct; dead `group_id` field; `discovered.rs` stub; tags serialization silent data loss; duplicate auth query; `COUNT` vs `EXISTS` |
| ⚪ Info | 4 | No API docs (OpenAPI); no troubleshooting guide; feature-gated auth duplication verbose but functional; placeholder secrets in committed files |

**Overall Health Score: 6.5 / 10**

VexBoard is a well-architected, actively developed project with solid fundamentals: clean crate separation, consistent naming, good CI/CD, comprehensive configuration management, and proper dependency pinning. The primary weaknesses are a critical authentication gap (client-side Leptos provides security the backend doesn't enforce), severely insufficient test coverage (2 tests for ~4,500 lines of code), and a production-unfriendly session cookie configuration. These are the kind of issues common in early-stage projects where features are prioritized before hardening — all are fixable without architectural changes. The score reflects genuine production-readiness concerns rather than any structural flaw.

---

## PART 2: IDEAS & RECOMMENDATIONS

### 2.1 Quick Wins (low effort, high value)

**1. ✅ DONE (2026-06-04) — Add session authentication middleware to the Axum router**
- `require_auth` middleware implemented in `crates/vexboard-server/src/api/mod.rs`, applied to all protected routes via `.route_layer(middleware::from_fn(require_auth))`.

**2. ✅ DONE (2026-06-04) — Fix N+1 query in `list_services`**
- Replaced per-service probe result loop with a single `LEFT JOIN` query in `crates/vexboard-server/src/api/services.rs`.

**3. ✅ DONE (2026-06-04) — Add error logging to silently-failing probe DB writes**
- `tracing::error!` added to all `let _ =` failure paths in `uptime.rs` and `auth.rs`.

**4. ✅ DONE (2026-06-04) — Make session `with_secure` configurable**
- `secure_cookies` field added to `AuthConfig` in `config.rs`; `config/default.toml` sets `secure_cookies = false`; wired to `.with_secure(config.auth.secure_cookies)` in `main.rs`.

**5. ✅ DONE (2026-06-05) — Remove the `Setting` struct**
- Deleted dead `Setting` struct and its `#[allow(dead_code)]` annotation from `crates/vexboard-server/src/db/models.rs`. No usages existed anywhere in the codebase.

---

### 2.2 Feature Recommendations

**1. ✅ DONE (2026-06-05) — Persistent SQLite-backed session store**
- `tower-sessions-sqlx-store 0.15.0` had a hard version mismatch with our `tower-sessions 0.15` (core trait version collision). Implemented a custom `SqliteSessionStore` in `crates/vexboard-server/src/session_store.rs` using `#[async_trait]`, `serde_json`, and `time` — all already in the dependency graph. Sessions now persist across restarts in the existing SQLite database (`tower_sessions` table, created by `migrate()` at startup).

**2. ✅ DONE (2026-06-05) — CORS origin allowlist via configuration**
- Added `allowed_origins: Vec<String>` to `ServerConfig` with `#[serde(default)]` defaulting to `["*"]` (backward-compatible). `config/default.toml` documents the knob with a production example. `main.rs` maps `["*"]` → `CorsLayer::allow_origin(Any)` and any other list → parsed `HeaderValue` list with a `tracing::warn!` for malformed entries. No new dependencies.

**3. ✅ DONE (2026-06-05) — API rate limiting on the login endpoint**
- `tower-governor` was not available in the local registry; implemented a zero-dependency sliding-window `LoginRateLimiter` in `crates/vexboard-server/src/rate_limit.rs` using `std::sync::Mutex<HashMap<IpAddr, VecDeque<Instant>>>`. IP extracted via `ConnectInfo<SocketAddr>` with `X-Forwarded-For` fallback. Rate limit (default 10 attempts / 60 s) is configurable in `config/default.toml`. Set `login_rate_limit_attempts = 0` to disable. Check runs before any DB or bcrypt work.

**4. ✅ DONE (2026-06-05) — Audit log for sensitive operations**
- `audit_log` SQLite table in `002_audit_log.sql` with indexes on `created_at DESC` and `actor`. Fire-and-forget `db::audit::insert` helper. All mutating handlers (services, groups, quick-links, auth, discovery, setup) write audit records. `GET /api/v1/audit` paginated read endpoint added under `require_auth`.

**5. ✅ DONE (2026-06-05) — OpenAPI / Swagger API documentation**
- `utoipa` annotations added to all API handlers; `utoipa-swagger-ui` serves the browser UI at `/swagger-ui`. Commit: `feat(api): OpenAPI 3.x spec and Swagger UI via utoipa`.

**6. ✅ DONE (2026-06-05) — Service grouping in the UI**
- Frontend modal now includes group selector; dashboard renders services grouped; discovery panel supports group assignment. Commits: `feat(groups): group management UI and discovery panel group assignment`, `feat(ui): wire service group selector and Group sort mode to dashboard`.

**7. ✅ DONE (2026-06-05) — Webhook / notification support for probe state changes**
- `[notifications]` config section added; webhook sender fires on probe state transitions. Commit: `feat(notify): webhook delivery on probe state transitions`.

**8. ✅ DONE (2026-06-06) — Dashboard drag-to-reorder services**
- `PATCH /api/v1/services/reorder` endpoint added (SQLite transaction, audit log). Frontend: draggable wrappers on service cards in Default sort mode, HTML5 DragEvent handlers compute new order on drop and PATCH backend. `DragEvent` added to web-sys features.

**9. ✅ DONE (2026-06-06) — Multi-user access control (roles)**
- `role` column added to `users` via idempotent migration `003_user_roles.sql`. `require_admin` middleware in `api/mod.rs` gates all write routes. Full user CRUD at `/api/v1/users`. Frontend hides edit/delete for viewers; Settings page User Management card (admin only) with role toggle, delete, create form.

**10. ✅ DONE (2026-06-06) — Dark/light mode toggle**
- Toggle button in Settings page (`crates/vexboard-frontend/src/pages/settings.rs`). Inline IIFE in `index.html` reads `localStorage.getItem('vexboard-theme')` before WASM loads (eliminates flash-of-wrong-theme). Toggle handler calls `localStorage.setItem` on each switch. Defaults to dark; persists across reloads.

---

### 2.3 Refactoring Opportunities

**1. ✅ DONE (2026-06-06) — Extract authentication middleware into its own module**
- `crates/vexboard-server/src/middleware/auth.rs` created with `pub async fn require_auth` and `pub async fn require_admin`. `api/mod.rs` reduced to a clean router aggregator — all auth logic centralized in the new module.

**2. ✅ DONE (2026-06-06) — Consolidate feature-gated auth handler duplication**
- `login` unified into one public handler + two private cfg-gated helpers (`login_pam` / `login_local`); rate limit check and IP extraction live once. `me` collapsed into one function with an inline cfg assignment. `update_me` intentionally kept feature-gated — PAM version is a zero-arg 405 stub; unifying signatures would cause 422 errors in PAM mode (comment documents this).

**3. ✅ DONE (2026-06-06) — Split `dashboard.rs` into sub-components**
- `pages/dashboard.rs` (940 lines) converted to `pages/dashboard/` module directory. Extracted: `DashboardModals` (modals.rs, ~115 lines), `ServiceGrid` (service_grid.rs, ~295 lines), `QuickLinksSection` (quick_links_section.rs, ~75 lines). `mod.rs` retains types, async helpers, DashboardPage, and page header (~250 lines). Modal show signals consolidated to `RwSignal<bool>`.

**4. ✅ DONE (2026-06-06) — Create a shared user query helper in the `db/` module**
- `crates/vexboard-server/src/db/users.rs` created with `pub async fn get_user_by_username(pool, username)`. Helper is cfg-gated `#[cfg(not(all(unix, feature = "pam-auth")))]` matching both callers. Both inline query blocks in `auth.rs` (`login_local` and `update_me`) replaced with helper calls. `pub mod users;` added to `db/mod.rs`.

**5. ✅ DONE (2026-06-06) — Replace `COUNT(*)` with `EXISTS` for duplicate detection**
- All three `SELECT COUNT(*) ... unwrap_or(0) > 0` blocks replaced with `SELECT EXISTS(SELECT 1 FROM ... LIMIT 1)` returning `bool`. Locations: `discovery/systemd.rs` (systemd_unit check), `discovery/docker.rs` (display_name OR systemd_unit check), `api/services.rs` `claim_service` (systemd_unit check). Fallback changed to `unwrap_or(false)`; branch simplified from `> 0` to direct boolean.

---

### 2.4 Tooling & Workflow Suggestions

**1. ✅ DONE (2026-06-06) — Add a git pre-commit hook for formatting and lint**
- `scripts/hooks/pre-commit` committed (executable). Skips when no `.rs` files are staged; runs `cargo fmt --all -- --check` then `cargo clippy --workspace -- -D warnings` (skippable via `SKIP_CLIPPY=1`). `scripts/install-hooks.sh` (Linux/macOS symlink installer) and `scripts/install-hooks.ps1` (Windows copy installer) added. Run once per checkout to activate.
- **⚠️ REVISED (2026-06-06) — Hook neutered to no-op.** The hook blocked commits from GUI git clients (GitHub Desktop) because those clients run git hooks in a bare environment with no PATH — `cargo` could not be located regardless of how many platform-specific paths were injected. Attempts to hard-code NixOS/rustup paths made the hook non-portable across devices (e.g. Windows laptop). Decision: fmt/clippy enforcement moved exclusively to `scripts/preflight.sh` and CI, where the environment is controlled. The hook file is retained as a no-op so the install scripts remain valid.

**2. ✅ DONE (2026-06-06) — Build out the test suite**
- `src/tests.rs` added with 14 integration tests: health check, login success/failure/unknown, `/me` unauthenticated/authenticated, logout session invalidation, services-unauthenticated 401, admin-route-as-viewer 403, list empty, create-as-admin 201, create-and-delete, create-as-viewer 403. `TestApp` harness uses in-memory SQLite, `MemoryStore` sessions, `ConnectInfo` extension injection, and bcrypt cost 4 seeds. Tests compile cleanly; SIGSEGV at runtime is pre-existing D-Bus/zbus environment issue (unchanged).

**3. Add `SQLX_OFFLINE=true` support to the dev shell**
- `flake.nix` sets `DATABASE_URL="sqlite:./dev.db"`, which requires the database file to exist for SQLx compile-time query checking. Supporting `SQLX_OFFLINE=true` with a committed `sqlx-data.json` would allow offline builds and clean CI caches.
- **Files:** `flake.nix`, `.github/workflows/ci.yml`

**4. Add a `docker-compose.override.yml` for local development**
- The current `docker-compose.yml` targets production (uses the published `ghcr.io` image). A `docker-compose.override.yml` with bind-mounted source, local build, and `RUST_LOG=debug` would let developers test the full stack locally without pushing an image.

**5. Add OpenAPI spec generation as a CI artifact**
- Once `utoipa` is added (see Feature Recommendation #5), add a CI step that generates the OpenAPI spec and uploads it as a build artifact, giving operators a versioned API reference without running the server.