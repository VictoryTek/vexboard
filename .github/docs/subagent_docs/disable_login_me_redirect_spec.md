# Disable-Login Setting Ignored by Frontend — Specification

## Current State Analysis

- The "Disable Login" toggle (`auth_mode_toggle` feature, already shipped) correctly stores `auth_mode = "none"` in the SQLite `settings` table and `main.rs:149-163` correctly applies it as a DB override at startup, overwriting `config.auth.mode` before the router is built.
- `api/mod.rs:27-55` (`api::router`) correctly reads `auth_mode` and skips attaching the `require_auth`/`require_admin` middleware layers on every protected route when `auth_mode == "none"`. This part of the feature works: all API routes are genuinely open once the mode takes effect.
- The bug is in `crates/vexboard-server/src/api/auth.rs:317-350` (`me` handler, backing `GET /api/v1/auth/me`). This handler is mounted under the **public** `auth::router()` (`api/mod.rs:61`), so it is never gated by `require_auth` in the first place — it decides authentication status itself by checking `session.get::<String>("username")`. It has no knowledge of `state.config.auth.mode` at all: it returns `401 Unauthorized` whenever the session has no `username`, unconditionally.
- In `"none"` mode nobody ever logs in (there is no login screen shown for a reason to use it), so no session ever has a `username`. This means `/api/v1/auth/me` returns 401 forever, in every request, regardless of `auth_mode`.
- The frontend's only consumer of this endpoint, `MainLayout`'s mount effect (`crates/vexboard-frontend/src/main.rs:67-102`), treats any 401 from `/me` as "must authenticate" and unconditionally redirects to `/login` (or `/setup`) via `window.location.set_href`. It has no alternate signal for "auth is disabled, stay here."
- Net effect: toggling "Disable Login" in Settings, and restarting so the DB override takes effect, does make every actual API route unauthenticated — but the SPA shell itself still force-redirects every visit to `/login` before the user can reach any page, because it never learns that auth is disabled. This exactly matches the reported symptom: the setting "doesn't work," survives updates and restarts, and the user is always sent back to the login screen.
- Secondary consequence: because `current_user` (populated only on a 200 from `/me`) never gets set in `"none"` mode, `is_admin()` (`crates/vexboard-frontend/src/pages/settings.rs:28-31`) is permanently `false` even for the person who enabled the mode — so even if they bypassed the redirect manually, the admin-only "Authentication" section that contains the toggle would be hidden, making it impossible to switch back to "Require Login" from the UI.

## Problem

`GET /api/v1/auth/me` must reflect the server's actual authentication requirement, not just literal session state. When `auth.mode == "none"`, the whole point is that no login ever happens and no route requires one — `/me` returning 401 in that state is a false signal that contradicts every other route's behavior, and the frontend acts on that false signal.

## Proposed Solution

Teach `me` to answer consistently with the rest of the router: when there is no session AND the running server's resolved `auth.mode` is `"none"`, return `200` with a synthetic user representing "no authentication in effect," instead of `401`. This is a one-file backend fix — no frontend change needed, since `MainLayout` already does the right thing on a 200 response (populates `current_user`, does not redirect).

### Backend change (`crates/vexboard-server/src/api/auth.rs`, `me` handler only)

- Keep the existing `Some(username)` branch untouched (still resolves role from DB/session as today — covers the case where `auth.mode` was `"none"` in the past, someone still has a stale session with a real username, etc.).
- In the `_` (no session) branch: if `state.config.auth.mode == "none"`, return `200` with:
  ```json
  { "user": { "username": "anonymous", "role": "admin", "auth_mode": "none", "dashboard_sort_mode": "az" } }
  ```
  - `role: "admin"` is deliberate, not a privilege escalation: `api/mod.rs` already grants full unauthenticated access to every admin-gated route when `auth_mode == "none"` (see `admin_protected` bypass, `api/mod.rs:51-55`). Reporting anything less than admin here would just hide UI (like the "Authentication" toggle needed to switch back to "Require Login") the backend already allows the user to exercise via direct API calls.
  - `dashboard_sort_mode` falls back to `"az"` (the existing default) since there is no real per-user identity to key a stored preference on in this mode.
  - Otherwise (session empty and mode is `"session"`, the normal case) keep returning `401` exactly as today.

### Frontend

No changes required. `MainLayout`'s effect already populates `current_user` from any 200 response and only redirects on 401 (`main.rs:70-99`); `user_menu.rs`'s `auth_mode == "local"` checks already correctly fall through to the PAM-style read-only branch for any non-`"local"` value, so a `"none"` value renders the existing "managed externally" notice rather than a broken password-change form — acceptable copy for this mode (no separate string needed; out of scope to add one).

## Implementation Steps

1. Modify the no-session branch of `me` in `crates/vexboard-server/src/api/auth.rs` to check `state.config.auth.mode` and return the synthetic `200` response described above when it is `"none"`; keep `401` for every other case.
2. No new imports, no config/schema/migration changes.

## Dependencies

None — internal-only change to an existing handler, no new crates. Exempt from Context7 lookup per CLAUDE.md Dependency Policy.

## Configuration Changes

None.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Reporting `role: "admin"` for the synthetic no-auth user could look like a privilege escalation on casual read | It isn't: `auth_mode == "none"` already grants unauthenticated callers full admin route access at the middleware layer (pre-existing, unchanged by this fix); this change only makes the frontend's displayed role match what the backend already permits. |
| A stray session with a stale `username` still exists from before the mode was switched to `"none"` | Untouched `Some(username)` branch still runs first and resolves the real role as before — this fix only changes behavior for the *no-session* case. |
| Someone relies on `/me` returning 401 as a signal elsewhere | Grepped frontend for all 401 handling; `main.rs`'s `MainLayout` effect is the only consumer of `/me`'s status code in the codebase. |

## Out of Scope

- Any change to how the DB override / restart-required flow works (`auth_mode_toggle` feature) — that part is already correct and unaffected.
- Adding a dedicated frontend copy string for `auth_mode == "none"` in `user_menu.rs`'s account modal — the existing PAM-style fallback text is serviceable and changing it is not needed to fix the reported bug.
