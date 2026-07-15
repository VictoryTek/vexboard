# Spec: Auto-identify as sole account when Disable Login is on

> **Revision:** after the initial sole-admin `/me` resolution was implemented,
> direction changed: rather than surfacing a resolved identity as if the user
> had logged in, Disable Login mode now hides the user avatar/menu entirely
> (matching how Homepage-style dashboards handle no-login mode) and simply
> grants admin access with no visible identity concept. The sole-admin `/me`
> resolution below is kept, but only as a personalization backend for
> `dashboard_sort_mode` — it is never surfaced as a displayed username. See
> "Frontend: hide UserMenu in Disable Login mode" at the end of this doc.

## Current State Analysis

`GET /api/v1/auth/me` (`crates/vexboard-server/src/api/auth.rs::me`):

```rust
match session.get::<String>("username").await {
    Ok(Some(username)) => { /* real user: role, dashboard_sort_mode from DB */ }
    _ if state.config.auth.mode == "none" => {
        // synthetic anonymous/admin, hardcoded dashboard_sort_mode: "az"
    }
    _ => 401
}
```

When `auth.mode == "none"` (set via the DB-backed "Disable Login" admin toggle,
`crates/vexboard-server/src/api/settings.rs`, applied at startup in `main.rs`)
and there is no session cookie, `/me` always returns a synthetic
`{ username: "anonymous", role: "admin", dashboard_sort_mode: "az" }`, regardless
of what accounts exist in the `users` table.

Route-level authorization (`api/mod.rs::router`) already bypasses
`require_auth`/`require_admin` entirely whenever `auth_mode == "none"` — every
request is already treated as admin. `/me`'s identity is consumed purely for
display (username in the UI) and per-user personalization (dashboard sort mode,
looked up by username in the `settings` key/value table).

## Problem

User confusion: with Disable Login on, they're shown as "anonymous" and lose
their personal dashboard sort preference, even though they have a real account
and are the only user on the instance. Confirmed via user Q&A: the desired
behavior is "regardless of whether Login is disabled or enabled, I should be
logged in as my user, not anonymous."

## Decision (confirmed with user)

When `auth.mode == "none"` and there is no session:
- If the `users` table has **exactly one** row, `/me` returns that user's real
  `username`, `role` (from the DB, as usual), and their real
  `dashboard_sort_mode` (looked up by their username) — i.e. behaves as if
  they were authenticated as that account.
- If the `users` table has **zero or more than one** row, behavior is
  unchanged: synthetic `anonymous`/`admin`/`az` (ambiguous — cannot guess
  which human is at the keyboard).

This does not touch authorization (`require_auth`/`require_admin` are already
bypassed in `"none"` mode) — it only changes what identity `/me` reports.

## Implementation Steps

1. `crates/vexboard-server/src/db/users.rs`: add
   `pub async fn get_sole_user(pool) -> Result<Option<User>, sqlx::Error>` —
   returns `Some(user)` only when `SELECT COUNT(*) FROM users` is exactly 1
   (single query: `SELECT id, username, password_hash, role, created_at FROM
   users LIMIT 2`, return `Some` only if exactly one row came back).
2. `crates/vexboard-server/src/api/auth.rs::me`: in the
   `_ if state.config.auth.mode == "none"` arm, first call
   `db::users::get_sole_user`. If `Some(user)`, build the same response shape
   as the authenticated branch (real username, `resolve_role`-equivalent role
   — role already comes from this same `users` row, so use `user.role`
   directly — and `dashboard_sort_mode` looked up by `user.username`). Only
   fall through to the existing synthetic `anonymous`/`admin`/`az` response
   when `get_sole_user` returns `None` (0 or 2+ users) or errors.
   PAM builds have no `users` table — `get_sole_user` is gated
   `#[cfg(not(all(unix, feature = "pam-auth")))]` and PAM's `"none"` arm keeps
   today's synthetic response unconditionally (PAM has no local account
   concept to resolve a sole user against).
3. Extracted `resolve_effective_user(state, session) -> Option<(username,
   role)>` shared by both `me()` and `update_sort_mode()`: prefers the real
   session, falls back to the sole local account only when `auth.mode ==
   "none"`. Without this, `PUT /me/sort-mode` still 401'd with no session even
   after `/me` started resolving a sole-user identity — the "Group" sort
   choice could never actually persist in Disable Login mode, since there was
   no session to save it under.
4. `crates/vexboard-server/src/tests.rs`: add cases —
   - `auth.mode == "none"`, exactly one user in DB, no session → `/me` returns
     that user's real username/role/sort mode.
   - `auth.mode == "none"`, zero users → synthetic anonymous (unchanged).
   - `auth.mode == "none"`, two users, no session → synthetic anonymous
     (unchanged; ambiguous case).

## Dependencies

None — no new crates, no external API surface.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** a second admin account added later silently reverts everyone to
  anonymous. **Mitigation:** this is the explicit, agreed fallback for the
  ambiguous case — acceptable because it's the same behavior as today, not a
  regression, and multi-user instances are expected to require login anyway.
- **Risk:** extra DB query on every unauthenticated `/me` call in `"none"`
  mode. **Mitigation:** single indexed lookup (`users` table is tiny on a
  self-hosted dashboard), same cost class as the existing per-request
  `resolve_role` DB read on the authenticated path.

## Frontend: hide UserMenu in Disable Login mode

`crates/vexboard-frontend/src/components/user_menu.rs::UserMenu` already
fetches `/api/v1/auth/me` for its own `MeResponse { username, auth_mode }`.
Wrap the component's existing markup (avatar trigger button + dropdown +
account-settings modal) in a `<Show when=move || me.get().map(|m| m.auth_mode
!= "none").unwrap_or(false)>`. When `auth_mode == "none"`, nothing renders —
no avatar, no username, no logout/account-settings entry point — consistent
with there being no login concept in that mode. `metric_bar.rs` mounts
`<UserMenu />` inside a `margin-left: auto` div that collapses cleanly when
empty, so no layout changes are needed there.
