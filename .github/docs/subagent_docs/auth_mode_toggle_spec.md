# Auth Mode Settings Toggle — Specification

## Current State Analysis

- `AuthConfig.mode` (`crates/vexboard-server/src/config.rs:61-65`) is `"session"` (default) or `"none"`, loaded once at process startup by `AppConfig::load()` via layered TOML files (`config/default.toml` → `/etc/vexboard/config.toml`) and `VEXBOARD_` env vars (env wins).
- `main.rs:232` bakes the mode into the Axum router at boot: `api::router(&config.auth.mode)` conditionally attaches `require_auth`/`require_admin` middleware layers (`api/mod.rs:27-62`). There is no per-request check — changing the value at runtime has no effect until the process restarts and rebuilds the router.
- `config.rs:189-194` enforces `auth.secret` must be ≥32 bytes whenever the *resolved* mode is `"session"`, failing fast at startup otherwise.
- **NixOS flake deployment blocker:** `nix/module.nix` declaratively generates `/etc/vexboard/config.toml` from module options, and the systemd unit runs with `ProtectSystem = "strict"` + `ReadWritePaths = [ cfg.dataDir ]` only (`nix/module.nix:159-160`) — `/etc/vexboard/` is read-only to the running process. Writing the toggle to the TOML config is therefore impossible on this deployment target, and even if it worked it would be overwritten on the next `nixos-rebuild`.
- The SQLite `settings` table already exists (`db/migrations/001_init.sql:45-48`, `key TEXT PRIMARY KEY, value TEXT NOT NULL`) but is currently unused anywhere in `src/`. The SQLite DB lives under `config.database.path`, inside `dataDir`, which **is** writable on every deployment type (Docker, manual, NixOS `ReadWritePaths`).
- Settings page (`crates/vexboard-frontend/src/pages/settings.rs`) already has an admin-only "User Management" section gated by `is_admin()`, following the same context/signal pattern the new section will reuse. No existing settings-write pattern exists in `api/config.rs` (currently read-only, unauthenticated `public_config`).
- `require_admin` middleware (`middleware/auth.rs`, wired in `api/mod.rs:14,61`) is the existing pattern for admin-only mutating routes (see `users::router()`).

## Problem

The user wants a Settings-page toggle to switch between `auth.mode = "session"` (login required) and `auth.mode = "none"` (open, network-gated trust model), without hand-editing config files or env vars. Per user decisions:
- A restart is an acceptable requirement to apply the change (no live router refactor).
- No special-case safeguard needed for the "none → session" direction (admin credentials already exist from setup / NixOS shares login with the OS account).
- The value must persist somewhere writable on **all** deployment types, including the NixOS flake, ruling out writing to the TOML config file.

## Proposed Solution

Store the toggle as a DB-backed **override** that takes precedence over the file/env config at the next startup, using the existing unused `settings` table (`key = "auth_mode"`, `value = "session" | "none"`).

### Startup resolution order (highest to lowest precedence)
1. DB `settings.auth_mode` (if present and valid) — the runtime toggle.
2. `VEXBOARD_AUTH__MODE` env var / `/etc/vexboard/config.toml` / `config/default.toml` (existing `AppConfig::load()` behavior, unchanged).

### Backend changes

1. **`db` module** — add `get_setting(pool, key) -> anyhow::Result<Option<String>>` and `set_setting(pool, key, value) -> anyhow::Result<()>` (upsert via `INSERT ... ON CONFLICT(key) DO UPDATE`) as small generic helpers over the existing `settings` table. No migration needed.

2. **`main.rs`** — reorder startup slightly: load `AppConfig` (raw, not yet `Arc`'d) → init DB pool (already happens next) → look up `settings.auth_mode`; if present and one of `"session"`/`"none"`, overwrite `config.auth.mode` with it (log at `info` that a DB override is in effect); if present but invalid, ignore and log a `warn`. **Re-run the same secret-length validation** currently in `config.rs:189-194` *after* applying the override — if the resolved mode is `"session"` and `auth.secret.len() < 32`, fail fast at startup with the same error message. This matters because a deployment that started life in `"none"` mode may never have set a real secret; silently starting an unauthenticated-by-accident `"session"` mode with a weak/default secret would be worse than refusing to boot. Then wrap in `Arc::new` as today.

3. **New endpoint** `PATCH /api/v1/settings/auth-mode` (admin-only, under the existing `admin_protected` router in `api/mod.rs`, same `require_admin` gate as `/api/v1/users`):
   - Body: `{"mode": "session" | "none"}`; reject anything else with 400.
   - Writes to `settings` via `set_setting`.
   - Returns `{"stored_mode": "...", "active_mode": "...", "restart_required": bool}` where `active_mode` is the mode the *running* process was actually built with (`state.config.auth.mode`, unaffected by this write) and `restart_required` is `stored_mode != active_mode`.

4. **Extend `GET /api/v1/config/public`** (or add an admin-only `GET /api/v1/settings/auth-mode`) to expose `active_mode` and `stored_mode` so the Settings page can render current state and the restart banner on load without guessing. Since `stored_mode` is only meaningful to an admin deciding on auth policy, put it behind `require_admin` as a separate small handler in `api/settings.rs` (new module) rather than widening the public endpoint.

### Frontend changes (`crates/vexboard-frontend/src/pages/settings.rs`)

- New admin-only section "Authentication" (same `<Show when=move || is_admin()>` pattern as User Management), with two options styled like the existing sidebar-mode radio buttons (`settings-nav-option` / `settings-nav-option-active`):
  - "Require Login" (`session`) — default, recommended.
  - "No Login (network-gated)" (`none`) — short description: only use this if your network already restricts access (e.g. Tailscale-only, isolated LAN).
- On mount (admin only), `GET` the new status endpoint to populate current `stored_mode`/`active_mode`.
- On selecting an option that differs from `stored_mode`, `PATCH /api/v1/settings/auth-mode`, then show/hide a persistent inline banner: *"Authentication mode changed — restart VexBoard for this to take effect."* whenever `stored_mode != active_mode`.
- No confirmation dialog needed per user's decision (credentials already exist / low lockout risk); keep it a plain click-to-select control consistent with the sidebar-mode UI already on the page.

## Implementation Steps

1. Add `get_setting`/`set_setting` helpers to `db` module.
2. Update `main.rs` startup sequence: apply DB override + re-validate secret length before building the router.
3. Add `api/settings.rs` with the `GET`/`PATCH` auth-mode handlers; register under `admin_protected` in `api/mod.rs`.
4. Add OpenAPI annotations consistent with other admin endpoints (`openapi.rs`) — required since Swagger UI (`/swagger-ui`) documents all routes today.
5. Add the "Authentication" section to `settings.rs` frontend page.
6. Update `README.md` / NixOS module docs if they currently describe `auth.mode` as file/env-only (check `README.md` auth.mode section during implementation).

## Dependencies

No new external crates. `config` crate, `sqlx`, `axum`, `leptos`, `gloo-net` already in use match the existing patterns being extended — no Context7 lookup needed (internal change only, per CLAUDE.md Dependency Policy exemption for "internal code changes with no new dependencies").

## Configuration Changes

None to `config/default.toml` schema. Behavior addition only: `AppConfig::load()`'s caller (`main.rs`) now consults the DB after loading, rather than `config.rs` itself changing.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Admin toggles to `"session"` while `auth.secret` is still short/default (left over from a `"none"` deployment) | Re-run existing secret-length validation post-override; fail fast at startup with the existing actionable error message rather than starting in a half-secure state. |
| Frontend/backend state drift (`stored_mode` says one thing, running process another) after a toggle but before restart | Explicit `restart_required` flag from the PATCH response + persistent banner computed from `stored_mode != active_mode` on every settings page load. |
| DB override present but corrupted/invalid value | Ignore with a `warn` log, fall back to file/env-resolved mode — never crash startup over a bad override value (contrast with the strict fail-fast for the *file/env* value, which is operator-authored and should be caught immediately). |
| NixOS module documentation implying config.toml is the only way to set `auth.mode` | Note the DB-override precedence in `README.md` during implementation so operators aren't confused when a stale TOML value doesn't seem to "stick." |

## Out of Scope

- Live (no-restart) auth mode switching — explicitly deferred per user decision; would require refactoring `api::router` to check mode per-request from shared state.
- Any additional confirmation/re-auth step for the none→session direction — explicitly declined by user.
