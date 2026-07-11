# SEC-8 — PAM Mode Grants Every OS Account Admin — Spec

## Current State Analysis

Three distinct problems in the `pam-auth` feature path:

1. **Every PAM-authenticating user is admin** — `login_pam`
   (`crates/vexboard-server/src/api/auth.rs:94-140`) hardcodes
   `session.insert("role", "admin".to_string())` for any user PAM successfully authenticates.
   There is no allowlist or role mapping; any valid OS account on the host gets full admin
   access to VexBoard. `me()` (`crates/vexboard-server/src/api/auth.rs:267-274`, PAM branch)
   also hardcodes `role = "admin".to_string()` to match.
2. **No account-validity check** — `authenticate_pam`
   (`crates/vexboard-server/src/pam_auth.rs:73-100`) calls `pam_sys::authenticate` but never
   calls `pam_sys::acct_mgmt` (the PAM API step that checks whether the account is expired,
   locked, or otherwise administratively disabled per `/etc/shadow` and PAM module policy).
   A correctly-authenticating-but-administratively-disabled OS account can still log in.
3. **Synchronous PAM call blocks the async runtime** — `authenticate_pam` is a blocking FFI
   call (network-bound in some PAM module configurations, e.g. LDAP/Kerberos backends) invoked
   directly inside the `async fn login_pam` (`crates/vexboard-server/src/api/auth.rs:87-88`)
   without `spawn_blocking`, blocking the calling Tokio worker thread for the duration —
   documented as up to ~2s on failure.

`pam_sys::acct_mgmt` is available and already re-exported at the crate root (`pub use
wrapped::*` in `pam-sys-0.5.6/src/lib.rs`), matching the existing `pam_sys::authenticate` /
`pam_sys::end` call style already used in `pam_auth.rs`.

## Problem Definition

PAM mode currently has no privilege separation (every OS account is admin), skips a mandatory
PAM lifecycle step (account validity), and performs a blocking syscall on an async executor
thread.

## Proposed Solution

1. Add `auth.pam_admin_users: Vec<String>` config (default empty list) — a plaintext allowlist
   of OS usernames that should receive the `admin` role; every other successfully-authenticated
   PAM user gets `viewer`.
2. Call `pam_sys::acct_mgmt(handle_ref, PamFlag::NONE)` after a successful `authenticate` call,
   requiring both to return `PamReturnCode::SUCCESS` before `authenticate_pam` returns `true`.
3. Wrap the `authenticate_pam(...)` call in `tokio::task::spawn_blocking`, since it's a
   synchronous FFI call that may block on I/O (LDAP/Kerberos-backed PAM modules).

## Implementation Steps

### 1. `crates/vexboard-server/src/config.rs` — add allowlist field

In `AuthConfig` (lines 47-72), add:
```rust
/// OS usernames that receive the admin role when authenticating via PAM.
/// All other successfully PAM-authenticated users get the viewer role.
/// Only read when the `pam-auth` feature is compiled in.
#[serde(default)]
pub pam_admin_users: Vec<String>,
```

Add a corresponding commented example to `config/default.toml` under `[auth]`.

### 2. `crates/vexboard-server/src/pam_auth.rs` — account validity check

In `authenticate_pam` (lines 73-100), after the existing `authenticate` call, replace the
tail:
```rust
let ret = pam_sys::authenticate(handle_ref, PamFlag::NONE);
let success = ret == PamReturnCode::SUCCESS;

pam_sys::end(handle_ref, ret);

success
```
with:
```rust
let ret = pam_sys::authenticate(handle_ref, PamFlag::NONE);
if ret != PamReturnCode::SUCCESS {
    pam_sys::end(handle_ref, ret);
    return false;
}

let acct_ret = pam_sys::acct_mgmt(handle_ref, PamFlag::NONE);
let success = acct_ret == PamReturnCode::SUCCESS;

pam_sys::end(handle_ref, acct_ret);

success
```

### 3. `crates/vexboard-server/src/api/auth.rs` — spawn_blocking + role mapping

In `login_pam` (lines 94-140), replace:
```rust
use crate::pam_auth::authenticate_pam;
if authenticate_pam(&payload.username, &payload.password) {
```
with a `spawn_blocking` call (owned copies of username/password, since the closure must be
`'static`):
```rust
use crate::pam_auth::authenticate_pam;
let username = payload.username.clone();
let password = payload.password.clone();
let authenticated = tokio::task::spawn_blocking(move || authenticate_pam(&username, &password))
    .await
    .unwrap_or(false);
if authenticated {
```
and change the hardcoded role insert:
```rust
if let Err(e) = session.insert("role", "admin".to_string()).await {
```
to:
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
if let Err(e) = session.insert("role", role.to_string()).await {
```
and the response body's `"role": "admin"` (line ~121) to `"role": role`.

Also update `me()` (lines 267-274, PAM branch), which currently hardcodes `role =
"admin".to_string()`: change to read the session's stored `"role"` the same way the non-PAM
branch already does (`session.get::<String>("role").await.ok().flatten().unwrap_or_else(||
"viewer".to_string())`), since PAM sessions now carry a real per-user role written at login.

## Dependencies

None new — `pam_sys::acct_mgmt` and `tokio::task::spawn_blocking` are both already available
(pam-sys is already a dependency gated by the `pam-auth` feature; tokio is already a core
dependency).

## Configuration Changes

New optional config key `auth.pam_admin_users` (`Vec<String>`, default `[]`). Documented in
`config/default.toml` under `[auth]`. Existing deployments without this key continue to work —
every PAM user simply becomes `viewer` until an admin adds their username to the list (safe
default, matches SEC-5's fail-closed-to-viewer precedent already established in this codebase).

## Risks and Mitigations

- **Risk:** Existing PAM deployments relying on "every OS user is admin" break on upgrade (all
  users become viewer).
  **Mitigation:** Intentional — this is the security fix. `auth.pam_admin_users` must be
  populated for admin access to continue; documented in `config/default.toml`.
- **Risk:** `acct_mgmt` failing for reasons unrelated to security (e.g. PAM module
  misconfiguration) could lock out previously-working accounts.
  **Mitigation:** This mirrors standard OS login behavior (e.g. `sshd`, `login` also call
  `pam_acct_mgmt`) — expected and correct PAM lifecycle usage, not a regression in intent.
- **Risk:** `spawn_blocking` failure (task panics or is cancelled) is mapped to `unwrap_or(false)`
  — fails closed (treated as authentication failure), which is correct.

## Test Plan

`cargo test -p vexboard-server` — this feature area (`pam-auth`) is not compiled or tested in
the default feature set (`cfg(all(unix, feature = "pam-auth"))`), and the existing test suite
already runs without the feature enabled, so no existing test is affected. No new automated
test is added — PAM integration testing requires a real PAM stack (test PAM service files,
actual OS accounts) that this environment and the existing test harness do not provide; this
matches the pre-existing lack of test coverage for `pam_auth.rs`. Compilation of the
`pam-auth`-gated code is verified via `cargo build --release --bin vexboard-server` (the
Approved safe build command list does not include a `--features pam-auth` variant per
Resource Constraints: "only compiles on Linux with `libpam-dev` present — do not enable it... in
cross-platform CI steps"; this environment's availability of `libpam-dev` is unconfirmed, so
feature-gated compilation is not exercised in Phase 3 — consistent with existing project
constraints).
