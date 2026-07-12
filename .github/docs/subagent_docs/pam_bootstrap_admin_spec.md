# PAM Bootstrap Admin Fallback — Spec

## Current State Analysis

PAM-mode role assignment lives entirely in `login_pam`
(`crates/vexboard-server/src/api/auth.rs:94-159`):

```rust
let role = if state
    .config
    .auth
    .pam_admin_users
    .iter()
    .any(|u| u == &payload.username)
{
    "admin"
} else {
    "viewer"
};
```

This was introduced deliberately by SEC-8
(`.github/docs/subagent_docs/pam-auth-hardening_spec.md`) to close a real vulnerability: prior
to that fix, every successfully-authenticated OS account was hardcoded to `admin`. The
allowlist (`auth.pam_admin_users`, default `[]` — `config/default.toml:42`) is the correct
long-term model and must not be weakened — an empty list must continue to mean "no one is
admin" for any *subsequent* login, otherwise every OS account on the box regains admin, which
is exactly the vulnerability SEC-8 closed.

The gap: a fresh PAM deployment with an empty (default/unconfigured) `pam_admin_users` list has
**no way to ever reach an admin session** through the app itself — there is no local `users`
table role in PAM mode (role is derived at login time from config, never persisted per-user),
so the "last admin" guards in `crates/vexboard-server/src/api/users.rs` don't apply and there is
no equivalent of the local-auth `setup::create_admin` bootstrap
(`crates/vexboard-server/src/api/setup.rs:77-158`, gated by `SELECT COUNT(*) FROM users`).
Confirmed real-world case: a NixOS flake install where `pam_admin_users` was never populated —
every PAM login, including the operator's only account, gets `viewer`, hiding the "+ Add"
button (`crates/vexboard-frontend/src/pages/dashboard/mod.rs:266`) and returning 403 from every
`admin_protected` route including `/api/v1/discovery/*`
(`crates/vexboard-server/src/api/mod.rs:44-53`), which is why the Discovered page renders empty.

## Problem Definition

PAM mode has no bootstrap path: an operator who never manually edits config before first login
can never become admin without directly editing config/restarting — there is no in-app recovery.

## Proposed Solution

Add a one-time, atomic, persisted bootstrap: when `auth.pam_admin_users` is empty, the **first
successful PAM login ever recorded** is granted `admin` and the grant is durably recorded so
every subsequent empty-list login goes back to `viewer` (matching SEC-8's intent — only one
implicit admin is ever created, not "every OS user until you configure the list").

Persistence: reuse the existing generic `settings` key/value table (`db/mod.rs:202-221`,
`get_setting`/`set_setting`, present since `001_init.sql` — no new migration needed) with a
single key `pam_bootstrap_admin`. Claim it atomically via a bare `INSERT` (not the existing
upsert-style `set_setting` helper, since `settings.key` is a `PRIMARY KEY` and a bare insert
will raise a `UNIQUE`/PK constraint error if the key already exists — that failure *is* the
concurrency guard, so two simultaneous first-logins can't both win):

```sql
INSERT INTO settings (key, value) VALUES ('pam_bootstrap_admin', ?)
```

Bind the claiming username as the value (useful for the audit trail / support debugging — "who
got the implicit bootstrap grant"). If the insert succeeds, this login is the bootstrap winner →
`admin`. If it fails specifically on the uniqueness constraint, another login already claimed
it → `viewer` (or continue normal allowlist logic if the list is non-empty by then — but since
we only attempt the claim when the list is empty, `viewer` is correct here).

This only ever fires while `pam_admin_users` is empty. The moment an operator populates the
list (per the SEC-8 doc's existing guidance), normal allowlist logic takes over unconditionally
and the bootstrap key becomes inert (never consulted, never cleared — no cleanup needed).

## Implementation Steps

### 1. `crates/vexboard-server/src/db/mod.rs` — atomic claim helper

Add near `get_setting`/`set_setting`:

```rust
/// Atomically claim a one-time settings flag. Returns `true` if this call performed the
/// insert (i.e. the flag was previously unset and is now claimed by `value`), `false` if
/// another caller already claimed it first. Used for the PAM bootstrap-admin grant, where
/// exactly one implicit admin may ever be created.
pub async fn try_claim_setting(pool: &SqlitePool, key: &str, value: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await;

    match result {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => Ok(false),
        Err(e) => Err(e.into()),
    }
}
```

### 2. `crates/vexboard-server/src/api/auth.rs` — bootstrap logic in `login_pam`

Replace the role computation block (around line 114-124) with:

```rust
let role = if !state.config.auth.pam_admin_users.is_empty() {
    if state
        .config
        .auth
        .pam_admin_users
        .iter()
        .any(|u| u == &payload.username)
    {
        "admin"
    } else {
        "viewer"
    }
} else if db::try_claim_setting(&state.db, "pam_bootstrap_admin", &payload.username)
    .await
    .unwrap_or(false)
{
    tracing::warn!(
        "auth.pam_admin_users is empty; granting one-time bootstrap admin to '{}'. \
         Set auth.pam_admin_users to make this permanent and stop further implicit grants.",
        payload.username
    );
    "admin"
} else {
    "viewer"
};
```

Note: `db::try_claim_setting` failing (`Err`, e.g. a DB error unrelated to the uniqueness
constraint) must fail closed to `viewer` via `unwrap_or(false)`, consistent with SEC-8's
existing fail-closed posture.

Add an audit log entry when the bootstrap grant fires (mirrors the existing
`setup.admin_created` event in `setup.rs:128-137`), placed after the existing
`db::audit::insert(..., "auth.login_success", ...)` call already present in `login_pam`:

```rust
if role == "admin" && state.config.auth.pam_admin_users.is_empty() {
    // Only true on the exact login that won the bootstrap claim above.
}
```

Simpler: fold this into the `try_claim_setting` success branch directly (emit the audit insert
there, alongside the `tracing::warn!`) rather than re-deriving it after the fact — avoids a
second condition that could drift out of sync with the claim outcome.

### 3. `config/default.toml` — document the new behavior

Extend the existing `pam_admin_users` comment block (`config/default.toml:38-42`):

```toml
# OS usernames that receive the admin role when authenticating via PAM (only
# relevant when the server is built with the `pam-auth` feature). Every other
# successfully PAM-authenticated user gets the viewer role.
#   pam_admin_users = ["alice", "bob"]
# Bootstrap: if this list is left empty, the very first successful PAM login
# (across the lifetime of this database) is granted a one-time admin role so a
# fresh install always has a way to reach the admin UI. Every login after that
# first one gets viewer until you populate this list explicitly.
pam_admin_users = []
```

## Dependencies

None new. Uses existing `sqlx`, existing `settings` table, existing `db::audit::insert`.

## Configuration Changes

None required from operators — this only changes behavior when `pam_admin_users` is left at
its default empty value, and only for the first login.

## Risks and Mitigations

- **Risk:** Two simultaneous first-ever logins race for the bootstrap grant.
  **Mitigation:** The claim is a single atomic `INSERT` against a `PRIMARY KEY` column — SQLite
  serializes this; exactly one caller observes success.
- **Risk:** Someone deletes the `pam_bootstrap_admin` settings row (e.g. manual DB edit),
  re-arming the bootstrap grant for the next login.
  **Mitigation:** Accepted — same trust model as any other direct DB edit; not a new attack
  surface (an attacker with DB write access already has full control).
- **Risk:** An operator intentionally wants "no one is ever auto-admin" even on a fresh
  install (e.g. air-gapped multi-tenant box where the very first login might not be trusted).
  **Mitigation:** Out of scope for this fix per explicit user decision — this environment's
  threat model is single-operator self-hosted, matching the existing SEC-8 doc's target
  deployment. Document clearly in `default.toml` so operators can consciously pre-populate
  `pam_admin_users` before first login to opt out.
- **Risk:** This reintroduces *some* implicit-admin surface that SEC-8 removed.
  **Mitigation:** Bounded to exactly one grant total (not "every user" as before SEC-8), and
  only while the list is empty — materially different risk profile, explicitly chosen by the
  user over the "empty list = every user admin" alternative.

## Test Plan

`cargo test -p vexboard-server` (existing suite; `pam-auth` feature code is not compiled or
tested in the default feature set per project constraints, consistent with SEC-8's own test
plan). Add a unit test for `db::try_claim_setting` in `crates/vexboard-server/src/tests.rs`
(feature-independent — it only touches the generic `settings` table, no PAM/FFI involved):
first call against a fresh in-memory pool returns `Ok(true)`; a second call with the same key
returns `Ok(false)`; the stored value matches the first caller's argument, not the second's.
