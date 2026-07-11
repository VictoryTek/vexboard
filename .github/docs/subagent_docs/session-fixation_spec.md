# SEC-3 — Session ID Rotation on Login — Spec

## Current State Analysis

`src/api/auth.rs` has two login paths, gated by the `pam-auth` feature:

- `login_pam` (crates/vexboard-server/src/api/auth.rs:94-140) — on successful PAM auth, calls
  `session.insert("username", ...)` and `session.insert("role", "admin")` directly on the
  `Session` extracted for the incoming (pre-auth) request.
- `login_local` (crates/vexboard-server/src/api/auth.rs:142-221) — on successful password
  verification, does the same two `session.insert(...)` calls.

Neither path calls `session.cycle_id()` before writing the authenticated identity into the
session. `tower_sessions_core::Session::cycle_id()` (confirmed present in tower-sessions-core
0.15.0, already a workspace dependency) regenerates the session's ID while preserving its
data, and is the documented tower-sessions primitive for this exact case.

## Problem Definition

Session fixation: an attacker who can get a victim to authenticate using an attacker-known
session ID (e.g. by setting the cookie via XSS on a subdomain, or a pre-auth session cookie
handed out by the server before login) can hijack the resulting authenticated session, because
the session ID does not change across the trust boundary crossed at login.

## Proposed Solution

Call `session.cycle_id().await` immediately before the `session.insert("username", ...)` /
`session.insert("role", ...)` calls in both `login_pam` and `login_local`, only on the
successful-authentication branch (never on the failure branches, which don't touch the
session's authenticated state anyway). Log and continue (matching the existing pattern for
`session.insert` errors) rather than failing the request if cycling errors, since the existing
code already tolerates `insert` failures the same way.

## Implementation Steps

1. In `login_pam` (crates/vexboard-server/src/api/auth.rs:102-108): after the
   `authenticate_pam(...)` success check, before the two `session.insert` calls, add:
   ```rust
   if let Err(e) = session.cycle_id().await {
       tracing::error!("failed to cycle session id after login: {e}");
   }
   ```
2. In `login_local` (crates/vexboard-server/src/api/auth.rs:195-200): same pattern, placed
   before the two `session.insert` calls (i.e. right after the `valid` password check passes).

No schema, config, or dependency changes are required.

## Dependencies

None new — `tower_sessions::Session::cycle_id` is already available via the existing
`tower-sessions` workspace dependency (tower-sessions-core 0.15.0).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** `cycle_id()` could fail (store error) leaving the old session ID in place.
  **Mitigation:** matches existing tolerance pattern used for `session.insert` errors in the
  same function — log and continue rather than blocking login on a session-store hiccup.
- **Risk:** None to functional behavior — cycling preserves session data, so subsequent
  `session.insert` calls in the same request still land on the (new) session.

## Test Plan

`cargo test -p vexboard-server` (existing `client_ip_tests` unaffected); no new automated test
is added since `cycle_id()` behavior is exercised by the tower-sessions-core crate itself and
there is no existing integration test harness for the login handlers in this codebase to hook
into without disproportionate new infrastructure (out of scope for this fix).
