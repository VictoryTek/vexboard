# Session Lifecycle Hardening — Spec (SEC-1)

Source: MASTER_PLAN.md HIGH PRIORITY / Security / SEC-1
(B-H2, B-H4, A-A1, A-A2)

## Current State Analysis

- `AuthConfig::secret` and `AuthConfig::session_ttl_hours` (`crates/vexboard-server/src/config.rs:40-41,48-49`)
  are deserialized from config but never read anywhere else in the codebase.
- `SessionManagerLayer` is constructed in `main.rs:208-209` with only `.with_secure(...)`.
  No `.with_expiry(...)` is set, so tower-sessions defaults to `Expiry::OnSessionEnd`
  (session cookie has no `Max-Age`; the *stored* session row also never expires because
  nothing ever recomputes/writes an earlier `expiry_date`). Combined with
  `SqliteSessionStore` never being purged (BUG-7, separate item), sessions are
  effectively permanent.
- Cookies are unsigned. `SessionManagerLayer` supports `.with_signed(Key)` /
  `.with_private(Key)` (tower-sessions 0.15, confirmed via Context7 docs and
  `examples/signed.rs` in the vendored crate source), gated behind the `signed`
  Cargo feature (`tower-cookies/signed`). Since `auth.secret` is never turned into a
  `cookie::Key`, the NixOS module's mandatory `secretFile`/`VEXBOARD_AUTH__SECRET`
  guard (`nix/module.nix:132-148`) blocks startup over a value the server ignores.
- `session_store.rs`'s `SqliteSessionStore` only implements the `SessionStore` trait
  (`save`/`load`/`delete` keyed by session ID). There is no way to look up or delete
  sessions by username, so `PATCH /api/v1/users/{id}` (`api/users.rs:182-305`) and
  `DELETE /api/v1/users/{id}` (`api/users.rs:325-413`) have no mechanism to revoke a
  target user's live sessions after a role change, rename, or deletion.
- Session data is a JSON object with at least `username` (String) and `role` (String)
  keys, set at login (`api/auth.rs` — both the local and PAM login paths call
  `session.insert("username", ...)` / `session.insert("role", ...)`).

## Problem Definition

1. Sessions never expire (neither cookie `Max-Age` nor server-side TTL enforcement),
   despite a configured `session_ttl_hours`.
2. Demoting, renaming, or deleting a user has no effect on that user's already-issued
   session(s) — they keep acting under the old identity/privileges until they manually
   log out.
3. `auth.secret` is configured, documented, and gated on by the NixOS module, but has
   zero effect on the running server (security theater) — cookies are unsigned, so a
   client could in principle tamper with cookie-adjacent data tower-sessions may rely on
   in future, and the "configure a secret" workflow currently protects nothing.

## Proposed Solution

### 1. Wire `session_ttl_hours` into cookie/store expiry

In `main.rs`, change:
```rust
let session_layer = SessionManagerLayer::new(session_store).with_secure(config.auth.secure_cookies);
```
to additionally call:
```rust
.with_expiry(Expiry::OnInactivity(time::Duration::seconds(
    config.auth.session_ttl_hours as i64 * 3600,
)))
```
This both sets the cookie `Max-Age` and causes tower-sessions to persist a rolling
`expiry_date` on each save, which `SqliteSessionStore::load` (`session_store.rs:79-81`)
already checks and rejects once passed. No change needed to `session_store.rs` for this
part — the expiry check already exists, it just never received a real expiry to enforce.

### 2. Make `auth.secret` real: sign session cookies

- Add the `signed` feature to the `tower-sessions` workspace dependency
  (`Cargo.toml:22`): `tower-sessions = { version = "0.15", features = ["signed"] }`.
- In `main.rs`, derive a `cookie::Key` from the configured secret and pass it via
  `.with_signed(key)`:
  ```rust
  use tower_sessions::cookie::Key;
  let key = Key::derive_from(config.auth.secret.as_bytes());
  let session_layer = SessionManagerLayer::new(session_store)
      .with_secure(config.auth.secure_cookies)
      .with_expiry(Expiry::OnInactivity(time::Duration::seconds(config.auth.session_ttl_hours as i64 * 3600)))
      .with_signed(key);
  ```
  (`Key::derive_from` HKDF-derives a proper signing/encryption key from any input of
  at least 32 bytes; panics below that — confirmed in vendored `cookie-0.18.1` source.)
- Add a length check in `AppConfig::load()` (`config.rs:163-184`), alongside the
  existing `auth.mode` validation, so a too-short/default secret fails fast with a
  clear message instead of panicking inside `Key::derive_from`:
  ```rust
  if app_config.auth.secret.len() < 32 {
      anyhow::bail!(
          "auth.secret must be at least 32 bytes (got {}); generate one with `openssl rand -base64 48`",
          app_config.auth.secret.len()
      );
  }
  ```
  This enforces the requirement for **every** deployment method (Docker, bare binary,
  NixOS), not just Nix. Since the bundled default (`config/default.toml:19`,
  `"change-me-in-production"`, 20 bytes) now fails this check, every deployment must
  set a real secret — matching the intent of the existing NixOS `preStart` guard.
  **No change to `nix/module.nix` is needed**: its guard already blocks startup when
  the secret is absent/placeholder, and that guard now corresponds to a real
  requirement instead of a no-op, so it stops being "security theater" without being
  touched.

### 3. Invalidate a user's live sessions on role change, rename, or deletion

- Add an inherent method to `SqliteSessionStore` (`session_store.rs`) — not part of the
  `SessionStore` trait, just a helper used from the API layer:
  ```rust
  pub async fn delete_by_username(&self, username: &str) -> Result<(), sqlx::Error> {
      let rows = sqlx::query("SELECT id, data FROM tower_sessions")
          .fetch_all(&self.pool)
          .await?;
      for row in rows {
          let id: String = row.try_get("id")?;
          let data: String = row.try_get("data")?;
          let matches = serde_json::from_str::<serde_json::Value>(&data)
              .ok()
              .and_then(|v| v.get("username").and_then(|u| u.as_str().map(str::to_string)))
              .is_some_and(|u| u == username);
          if matches {
              sqlx::query("DELETE FROM tower_sessions WHERE id = ?")
                  .bind(id)
                  .execute(&self.pool)
                  .await?;
          }
      }
      Ok(())
  }
  ```
  A full-table scan is acceptable here: the `tower_sessions` table holds one row per
  active session for a self-hosted dashboard, and this only runs on admin
  role/username/delete actions, not per-request.
- Add `pub session_store: session_store::SqliteSessionStore` to `AppState`
  (`main.rs:113-120`) so API handlers can reach it. Construct it once in `main()` and
  clone into both the layer and the state (it already derives `Clone`).
- In `api/users.rs::update_user`, after the `UPDATE users ...` succeeds, if the
  username or role actually changed, call
  `let _ = state.session_store.delete_by_username(&target.username).await;`
  using `target.username` (the *pre-update* username, already fetched earlier in the
  handler) as the lookup key, since that's what's stored in the live session(s).
  Log (`tracing::warn!`) but do not fail the request if this errors — invalidation is
  a best-effort hardening measure, not the primary write.
- In `api/users.rs::delete_user`, after the `DELETE FROM users` succeeds, call the
  same `delete_by_username(&target.username)`.

## Implementation Steps

1. `Cargo.toml` — add `features = ["signed"]` to the `tower-sessions` dependency.
2. `crates/vexboard-server/src/config.rs` — add the `auth.secret` minimum-length check
   in `AppConfig::load()`.
3. `crates/vexboard-server/src/session_store.rs` — add `delete_by_username`.
4. `crates/vexboard-server/src/main.rs` — add `session_store` to `AppState`; build the
   `Key`; chain `.with_expiry(...)` and `.with_signed(key)` onto the session layer.
5. `crates/vexboard-server/src/api/users.rs` — call `delete_by_username` from
   `update_user` (on role or username change) and `delete_user`.
6. `config/default.toml` — update the `secret` comment/placeholder to note the new
   32-byte minimum (no functional change, just keeps the file's own docs accurate).

## Dependencies

- `tower-sessions` 0.15.0 (already in the workspace, `Cargo.lock` confirmed) — adding
  the `signed` feature only, no version change. API verified via Context7
  (`/maxcountryman/tower-sessions`) and the crate's own vendored `examples/signed.rs`:
  `SessionManagerLayer::with_signed(cookie::Key)`, `Expiry::OnInactivity(time::Duration)`.
- `cookie` 0.18.1 was already present transitively (via `tower-cookies`), but
  `Key::derive_from` requires its `key-expansion` feature (gated separately from
  `signed`/`private`; confirmed in the vendored `cookie-0.18.1` source). Added `cookie`
  as an explicit workspace dependency (`default-features = false, features =
  ["key-expansion"]`) purely for Cargo feature unification — no new code depends on
  the `cookie` crate directly; `tower_sessions::cookie::Key` (the re-exported same
  type) is used throughout.
  `Key::derive_from(&[u8]) -> Key` HKDF-derives a 64-byte signing+encryption key,
  panics if input `< 32` bytes; guarded by the new config-level length check so the
  panic path is unreachable in practice.

## Configuration Changes

- `auth.secret` now has a hard minimum length of 32 bytes, enforced at startup
  (`AppConfig::load()`) **only when `auth.mode == "session"`**. Deployments still
  using the bundled placeholder under `mode = "session"` will fail to start with a
  clear error instead of silently running with an ineffective secret.
  `auth.mode == "none"` deployments (added separately, pre-existing feature —
  network-gated setups that skip login entirely) never exercise the login/session
  flow, so they're exempt from the requirement: `main.rs` falls back to an
  ephemeral, randomly generated `cookie::Key` when the configured secret is too
  short, rather than requiring users who explicitly opted out of login to still
  configure a signing secret they'll never use.
- `auth.session_ttl_hours` now actually bounds session lifetime (cookie `Max-Age` +
  server-side expiry), rolling on inactivity.

## Risks and Mitigations

- **Risk:** Existing deployments with a short/default `auth.secret` will fail to start
  after this change. **Mitigation:** This is intentional — the current state is
  broken (silent no-op secret); the failure is fail-fast and the error message
  includes the exact remediation command (`openssl rand -base64 48`), matching the
  existing NixOS guard's guidance.
- **Risk:** Enabling cookie signing changes the cookie payload; any session created
  before this change won't be recognized as a valid signed cookie. **Mitigation:**
  Acceptable — worst case is an unsigned pre-existing session is rejected and the user
  is redirected to log in again, which is a safe failure mode.
- **Risk:** `delete_by_username` full-table scan could be slow with a very large
  sessions table. **Mitigation:** Out of scope for this self-hosted, low-user-count
  dashboard; BUG-7 (expired-session cleanup, separate ticket) will additionally keep
  the table small over time.
- **Risk:** Self-service username/password change (`PATCH /api/v1/auth/me`) is a
  separate code path (`api/auth.rs::update_me`) not touched here — a user who renames
  themselves keeps their own current session (expected: they're still logged in as
  themselves) but SEC-1 only covers *admin-initiated* changes to *other* users per the
  master-plan bug description. No change needed there.

## Files

- `Cargo.toml:22`
- `crates/vexboard-server/src/config.rs:163-184`
- `crates/vexboard-server/src/session_store.rs`
- `crates/vexboard-server/src/main.rs:113-120,205-218`
- `crates/vexboard-server/src/api/users.rs:182-305,325-413`
- `config/default.toml:18-19`
