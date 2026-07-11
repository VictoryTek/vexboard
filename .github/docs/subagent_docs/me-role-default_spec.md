# SEC-5 — `/auth/me` Defaults Missing Role to "admin" — Spec

## Current State Analysis

`me()` (crates/vexboard-server/src/api/auth.rs:270-293), in the non-PAM (`local`) branch, reads
the `role` value out of the session:

```rust
let (role, auth_mode) = (
    session
        .get::<String>("role")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "admin".to_string()),
    "local",
);
```

If the `"role"` key is absent from the session — which is possible for sessions created before
roles existed in the session schema, or if `session.insert("role", ...)` silently failed on
login (both `login_pam` and `login_local` already log-and-continue on `insert` errors rather
than failing the request, per `crates/vexboard-server/src/api/auth.rs:103-108,199-204`) — this
falls back to `"admin"`. A user with an ambiguous/missing role is granted the most privileged
role by default. This is the only occurrence of this fallback pattern in the codebase (verified
via `grep -rn 'unwrap_or_else(|| "admin"'`).

## Problem Definition

Fail-open privilege default: a missing role value should never be interpreted as full admin
access. The safe default is the least-privileged role, `"viewer"`.

## Proposed Solution

Change the fallback string from `"admin"` to `"viewer"`.

## Implementation Steps

1. In `crates/vexboard-server/src/api/auth.rs:283`, change:
   ```rust
   .unwrap_or_else(|| "admin".to_string()),
   ```
   to:
   ```rust
   .unwrap_or_else(|| "viewer".to_string()),
   ```

No other call site uses this fallback pattern.

## Dependencies

None.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** A legitimate admin whose session predates the `role` key (or whose `insert("role",
  ...)` failed on login) would see themselves demoted to viewer in the `/me` response until
  they log in again.
  **Mitigation:** This is the intended, safe behavior — fail-closed rather than fail-open. The
  underlying middleware/route guards (`src/middleware/auth.rs`, viewer/admin-protected routers)
  also read role from the session, so this change is consistent with denying elevated access
  when the role cannot be positively confirmed. Affected users simply re-authenticate to get a
  session with the role properly set.

## Test Plan

`cargo test -p vexboard-server` — existing test `test_me_authenticated_returns_username_and_role`
already asserts the happy path where `role` is present and set correctly; it is unaffected by
this change since it doesn't exercise the missing-role fallback. No new test is added: covering
the missing-role case would require directly manipulating the session store to omit the `role`
key, which the existing test harness (`crate::tests`) doesn't currently support without
disproportionate new infrastructure for a one-line default-value fix.
