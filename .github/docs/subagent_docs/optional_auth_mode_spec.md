# Optional Auth Mode — Specification

## Current State Analysis

Authentication is session-cookie based (`tower_sessions::Session`, SQLite-backed store), enforced via two `axum::middleware::from_fn` layers defined in `crates/vexboard-server/src/middleware/auth.rs`:

- `require_auth` (lines 9-18): passes if `session.get::<String>("username")` is `Some`; else 401.
- `require_admin` (lines 20-39): additionally requires `session.get::<String>("role") == "admin"`; else 401/403.

These are wired into the router in `crates/vexboard-server/src/api/mod.rs`:
- `viewer_protected` (lines 23-29): `.route_layer(middleware::from_fn(require_auth))` over services/groups/quick-links (read), metrics, audit.
- `admin_protected` (lines 32-38): `.route_layer(middleware::from_fn(require_admin))` over services/groups/quick-links (mutating), discovery, users.
- A third, unmerged `Router::new()` block holds public routes (setup, auth, health, public config, swagger).

There is currently no config flag or mechanism to bypass either layer. `AuthConfig` (`crates/vexboard-server/src/config.rs` lines 46-61) holds `secret`, `session_ttl_hours`, `secure_cookies`, and login rate-limit fields — no `mode`/`enabled` field exists today.

No Docker/systemd start-stop-restart capability exists in the codebase (discovery is read-only), so the risk surface for disabling auth is limited to: viewing services/groups/quick-links/metrics/audit, and admin CRUD on services/groups/quick-links/users, plus triggering discovery rescans.

## Problem Definition

The user runs VexBoard on an isolated home LAN (IoT devices VLAN-separated, no other LAN occupants) and accesses it remotely exclusively via Tailscale (WireGuard-authenticated, tied to their tailnet identity). In this deployment, the login screen is friction with no corresponding security benefit — the network boundary is already the trust boundary, matching the model used by comparable self-hosted dashboards (e.g. Homepage) that assume the operator gates access at the network/reverse-proxy layer.

The request is to make authentication skippable for this deployment, without deleting/weakening auth for users who don't have an equivalent network boundary (default should stay secure).

## Proposed Solution Architecture

Add an `auth.mode` config field (default `"session"`, opt-in value `"none"`), read once at startup, that controls whether `require_auth`/`require_admin` are applied to the router. No new dependency is introduced — this uses only the existing `config`/`serde` stack already in the workspace, so Context7 verification does not apply (internal-only change, no new external library).

Design choice: rather than making the middleware functions themselves branch on config per-request (extra indirection, extra state threading), branch once at router-build time in `api::router()`, since `AppConfig` is already available where the router is constructed. This keeps the request-time hot path identical to today when auth is enabled, and reduces the "none" path to skipping two `.route_layer(...)` calls.

### Behavior when `auth.mode = "none"`

- `require_auth` and `require_admin` layers are not applied — all `/api/v1/*` routes (viewer + admin tier) become reachable without a session.
- The `setup` flow (first-admin creation) and `auth::router()` (login/logout/me) stay mounted and functional but become moot for gating purposes; they are left as-is rather than special-cased, since removing them would be scope creep and users may still want to log in for auditability of *who* made a change even when the gate itself is open — however per Simplicity First, we are not adding "act as this user when in none mode" behavior; audit log `actor` fields will simply show unauthenticated/absent-session entries. This is called out under Risks below rather than solved.
- The SPA static-asset fallback in `main.rs` is already unauthenticated today and needs no change.
- A one-line startup log (`warn!`) is emitted so operators can tell from logs which mode is active — no reliance on remembering config.

### Non-goals

- No per-route granularity (e.g. "no auth for reads, still require for admin") — out of scope; the user asked for full removal, and partial modes add complexity not requested.
- No IP allowlisting or network-based auth bypass — the user's own network boundary (Tailscale/VLAN) is what they're relying on; the app doesn't need to reimplement it.
- No change to PAM-auth feature — orthogonal (`auth.mode` governs whether login is required at all; `pam-auth` governs how login is verified when it is required).

## Implementation Steps

1. **`crates/vexboard-server/src/config.rs`** — add to `AuthConfig`:
   ```rust
   /// Authentication mode: "session" (default, login required) or "none"
   /// (all API routes open — only safe when the network boundary itself
   /// restricts access, e.g. Tailscale-only / isolated LAN).
   #[serde(default = "default_auth_mode")]
   pub mode: String,
   ```
   with `fn default_auth_mode() -> String { "session".to_string() }`.

2. **`config/default.toml`** — add `mode = "session"` under `[auth]` with a comment explaining `"none"` and the security tradeoff (mirrors the existing comment style for `secure_cookies`). The comment must also state the reverse path explicitly, e.g.:
   ```toml
   # Authentication mode:
   #   "session" (default) — login required; safe for any network exposure.
   #   "none" — all API routes open, no login required. Only safe when the
   #     network layer itself restricts access (e.g. Tailscale-only, isolated
   #     LAN with no untrusted devices). To re-enable login, set this back to
   #     "session" (or delete the line / unset VEXBOARD_AUTH__MODE) and
   #     restart the server — existing user accounts are untouched by mode
   #     changes in either direction, so no re-setup is needed.
   mode = "session"
   ```
   This ensures an operator who enabled `"none"` months ago and forgets the mechanism can find the exact revert step (flip the value, unset the env override if using one, restart) directly in the file, without needing to consult external docs.

3. **`crates/vexboard-server/src/api/mod.rs`** — change `router()` to accept the auth mode (or full `AppConfig`/relevant slice) as a parameter, and conditionally apply `.route_layer(...)`:
   ```rust
   pub fn router(auth_mode: &str) -> Router<AppState> {
       let viewer = Router::new()
           .nest(...)
           ...;
       let viewer_protected = if auth_mode == "none" {
           viewer
       } else {
           viewer.route_layer(middleware::from_fn(require_auth))
       };
       // same pattern for admin_protected
       ...
   }
   ```
   Exact call-site signature change to be confirmed by grepping `api::router()` call sites in `main.rs` before implementation (Phase 2 must locate and update the caller).

4. **`main.rs`** — update the call site to pass `config.auth.mode.as_str()` (or equivalent), and emit `tracing::warn!("auth.mode = \"none\": all API routes are unauthenticated; only use this if the network layer restricts access")` when `mode == "none"`, once at startup.

5. Reject unrecognized `mode` values at startup (fail fast with a clear error) rather than silently defaulting — use a simple match with an `anyhow::bail!` for anything other than `"session"` or `"none"`, matching existing `AppConfig::load()` error-handling style (returns `anyhow::Result`).

## Dependencies

None new. Uses existing `serde`, `config`, `axum`, `tracing` (already in the workspace `Cargo.toml`/`crates/vexboard-server/Cargo.toml`). No Context7 lookup required under the project's documented exemption ("Internal code changes with no new dependencies").

## Configuration Changes

- `config/default.toml`: new `[auth].mode = "session"` line (default, no behavior change for existing deployments).
- New env override: `VEXBOARD_AUTH__MODE=none` (consistent with existing `VEXBOARD_` prefix + `__` separator convention already used for `VEXBOARD_AUTH__SECRET`).
- No migration needed — SQLite schema untouched, existing sessions/users unaffected.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| User enables `mode = "none"` on a deployment that is *not* actually network-isolated (e.g. port-forwarded router) | Config comment explicitly states the precondition; startup `warn!` log makes the mode visible every boot; default stays `"session"` so this requires an explicit opt-in action |
| Audit log entries lose attribution (no session ⇒ no `actor`) when `mode = "none"` | Documented as an accepted tradeoff in this spec (Non-goals); not solved, since the user did not request it and it would require re-adding an identity concept to a mode whose whole point is removing that friction |
| Someone copies `default.toml`'s `mode = "session"` default but sets `VEXBOARD_AUTH__MODE` to a typo'd value (e.g. `"noauth"`) | Fail-fast validation at startup (`anyhow::bail!`) rather than silently treating anything-not-"none" as session-mode, so misconfiguration is loud, not silently secure-by-accident or silently open |
| Future contributor adds a new protected route and forgets it needs to also be skippable in `"none"` mode | Because the branch is on the two existing `route_layer` wrapping points rather than per-route, any route added to `viewer_protected`/`admin_protected` groups is automatically covered — no per-route bookkeeping required |

## Approved Validation Commands (per Phase 1 policy)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`
- `cargo audit --ignore RUSTSEC-2023-0071` (if installed)

No FORBIDDEN COMMANDS are required for this change (no WASM frontend touched, no workspace-wide build needed).
