# Spec: Fix dashboard sort mode resetting to A-Z with login disabled

## Current State Analysis

Sort mode persistence flow:

- **Frontend** (`crates/vexboard-frontend/src/pages/dashboard/mod.rs`):
  - `fetch_sort_mode()` (`:46-61`) — on dashboard mount, `GET /api/v1/auth/me`,
    reads `user.dashboard_sort_mode`, defaults to `SortMode::AZ` on any
    failure/missing field.
  - `save_sort_mode()` (`:63-74`) — on user change, `PUT
    /api/v1/auth/me/sort-mode`, response discarded (`let _ =
    req.send().await;`).
- **Server** (`crates/vexboard-server/src/api/auth.rs`):
  - `resolve_effective_user()` (`:316-330`) — prefers session username; if no
    session and `auth.mode == "none"`, falls back to
    `db::users::get_sole_user()` (`crates/vexboard-server/src/db/users.rs:16-30`),
    which returns `Some` only when the `users` table has **exactly one** row
    (`SELECT ... LIMIT 2`, checks `rows.len() == 1`). Zero or 2+ rows → `None`.
  - `me()` (`:343-387`) — when `resolve_effective_user` returns `Some`, reads
    the real `dashboard_sort_mode:{username}` setting. When it returns `None`
    and `auth.mode == "none"` (`:373-381`), returns a **hardcoded** synthetic
    response: `username: "anonymous"`, `dashboard_sort_mode: "az"` — never
    touches the `settings` table.
  - `update_sort_mode()` (`:583-617`) — when `resolve_effective_user` returns
    `None` (`:588-596`), returns `401 Unauthorized` unconditionally; the write
    never happens.
  - Storage is a generic KV table: `settings(key, value)`
    (`crates/vexboard-server/src/db/mod.rs:214-233`), keyed
    `dashboard_sort_mode:{username}`.

This "exactly one account" resolution and the "2+ accounts → anonymous, az"
fallback were an intentional prior decision
(`.github/docs/subagent_docs/disable_login_sole_admin_spec.md`), made for
**display identity** (avoiding showing the wrong username). That decision was
explicitly noted as carrying risk: *"a second admin account added later
silently reverts everyone to anonymous."* That risk has now materialized as a
real bug: any instance with zero or more than one local account, running with
login disabled, gets `dashboard_sort_mode: "az"` hardcoded on every `GET /me`
and a `401` on every `PUT /me/sort-mode` — so the sort choice appears to apply
in the UI (optimistic client-side update) but is silently never saved, and
reverts to A-Z on the next page load/navigation. Confirmed by existing test
`test_me_auth_mode_none_falls_back_to_anonymous_with_multiple_users`
(`crates/vexboard-server/src/tests.rs:445-455`), which currently asserts this
exact hardcoded-`az` behavior as correct.

## Problem

With login disabled, dashboard sort mode does not persist server-side unless
the instance happens to have exactly one local account. The user's
expectation — "set once, persists regardless of device/browser" — is
correct and achievable: when there's no session, there's no
per-device/per-browser concept at all, so a single shared, instance-wide sort
preference is the right model, independent of how many local accounts exist.

## Decision

Keep the existing **display identity** resolution unchanged (sole-user
username shown when exactly one account exists; "anonymous" otherwise — that
is a separate, already-agreed concern, not part of this fix).

Decouple **sort-mode persistence** from that identity resolution. When
`auth.mode == "none"` and there is no session (i.e. `resolve_effective_user`
returns `None`), read/write `dashboard_sort_mode` under a fixed,
instance-wide settings key instead of failing or hardcoding `"az"`. This
makes persistence work uniformly regardless of account count (0, 1, or many).

## Implementation Steps

1. `crates/vexboard-server/src/api/auth.rs`: add a constant near
   `resolve_effective_user`:
   ```rust
   const ANONYMOUS_SORT_MODE_KEY: &str = "dashboard_sort_mode:__anonymous__";
   ```
2. `me()` (`:373-381` arm): replace the hardcoded `"dashboard_sort_mode":
   "az"` with a real lookup:
   ```rust
   let dashboard_sort_mode =
       db::get_setting(&state.db, ANONYMOUS_SORT_MODE_KEY)
           .await
           .ok()
           .flatten()
           .unwrap_or_else(|| "az".to_string());
   ```
   Keep `username: "anonymous"` and `role: "admin"` as-is — display identity
   is out of scope for this fix.
3. `update_sort_mode()` (`:588-596`): instead of returning `401` whenever
   `resolve_effective_user` is `None`, fall back to the fixed key when login
   is disabled, and only 401 when it isn't:
   ```rust
   let key = match resolve_effective_user(&state, &session).await {
       Some((u, _)) => format!("dashboard_sort_mode:{u}"),
       None if state.config.auth.mode == "none" => {
           ANONYMOUS_SORT_MODE_KEY.to_string()
       }
       None => {
           return (
               StatusCode::UNAUTHORIZED,
               Json(json!({"error": "Not authenticated"})),
           )
       }
   };
   ```
   Then use `key` in place of the current `format!("dashboard_sort_mode:{username}")`
   at `:605`.
4. `crates/vexboard-server/src/tests.rs`:
   - Update `test_me_auth_mode_none_falls_back_to_anonymous_with_multiple_users`
     (`:445-455`): still assert `username == "anonymous"`, but no longer
     assert a hardcoded `"az"` — instead assert the value round-trips (see
     next point).
   - Add a new test: with `auth.mode == "none"` and two (or zero) local
     accounts, no session: `PUT /me/sort-mode` with `{"sort_mode": "group"}`
     returns `200`, and a subsequent `GET /me` reflects
     `dashboard_sort_mode: "group"`.
   - Keep the existing single-account test
     (`test_me_auth_mode_none_resolves_sole_user`, `:422-439`) unchanged — it
     covers the separate, already-correct per-username path.

No frontend changes are required — `fetch_sort_mode`/`save_sort_mode`
already call the same two endpoints; they'll now round-trip correctly once
the server-side key resolution is fixed.

## Dependencies

None — no new crates, no external API surface.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** on an instance with login disabled and multiple real accounts,
  sort mode becomes a single shared instance-wide preference rather than
  per-account. **Mitigation:** this is unavoidable and correct — with no
  session there is no way to distinguish which account is "at the keyboard,"
  so per-account preference has no valid meaning in that state; a shared
  preference is strictly better than the current silently-broken behavior.
- **Risk:** migrating from the old hardcoded-`az` behavior — any group/source
  sort chosen previously while login was disabled with 2+ accounts was never
  actually saved, so the first real save under the new fixed key starts from
  `az` (no prior data to lose).
