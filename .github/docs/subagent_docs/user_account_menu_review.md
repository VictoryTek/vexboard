# Review: User Account Menu — `user_account_menu`

**Reviewer:** QA Subagent  
**Date:** 2026-05-30  
**Spec:** `.github/docs/subagent_docs/user_account_menu_spec.md`  
**Verdict:** ⛔ NEEDS_REFINEMENT

---

## 1. Build Results

| Command | Exit Code | Result |
|---------|-----------|--------|
| `cargo build --release --bin vexboard-server` | **0** | ✅ PASS |
| `cargo clippy --workspace -- -D warnings` | **0** | ✅ PASS |
| `cargo test --workspace` | **101** | ⚠️ PRE-EXISTING (SIGSEGV in test binary — confirmed present before these changes via `git stash` test) |
| `cargo fmt --all -- --check` | **1** | ❌ FAIL — formatting diffs in `user_menu.rs` and `auth.rs` |
| `trunk build --release` | *not run* | Frontend WASM build not attempted (Trunk CLI required separately) |

### Backend Build (Release)
```
   Compiling vexboard-server v0.1.0
    Finished `release` profile [optimized] target(s) in 1m 01s
```
Exit 0 — clean compilation.

### Clippy
```
    Checking vexboard-server v0.1.0
    Checking vexboard-frontend v0.1.0
    Finished `dev` profile in 15.86s
```
Exit 0 — no warnings or lints.

### Tests
```
error: test failed, to rerun pass `-p vexboard-server --bin vexboard-server`
Caused by: process didn't exit successfully (signal: 11, SIGSEGV)
```
Exit 101 — **pre-existing failure** confirmed by stashing new changes and re-running; the SIGSEGV occurred identically on the prior commit. Not introduced by this feature.

### Format Check
```
Diff in user_menu.rs:10  — fetch_me() chain formatting
Diff in user_menu.rs:39  — web_sys::window() chain
Diff in user_menu.rs:78  — web_sys::window() chain (in Timeout closure)
Diff in user_menu.rs:85  — r.text().await chain
Diff in auth.rs:192      — sqlx::query_scalar() chain formatting
```
Exit 1 — **5 formatting diffs** across `user_menu.rs` (4) and `auth.rs` (1). Fix: `cargo fmt --all`.

---

## 2. Code Review Checklist

### Backend (`auth.rs`)

| Item | Status | Notes |
|------|--------|-------|
| `GET /api/v1/auth/me` returns `auth_mode` field | ✅ PASS | Returns `"auth_mode": "local"` or `"pam"` via `#[cfg]` |
| `PATCH /api/v1/auth/me` exists | ✅ PASS | `.route("/me", get(me).patch(update_me))` in router |
| PAM guard (405) on `PATCH` in PAM mode | ✅ PASS | `#[cfg(all(unix, feature = "pam-auth"))]` stub returns 405 |
| `current_password` verified via bcrypt before any change | ✅ PASS | `bcrypt::verify()` called before any DB mutation |
| Username uniqueness checked on rename (409 if conflict) | ✅ PASS | `SELECT id … WHERE username = ? AND id != ?` → 409 if `taken.is_some()` |
| `session.flush()` called on successful credential change | ✅ PASS | `session.flush().await.ok()` at end of success path |
| Route wired in router (`.patch(update_me)`) | ✅ PASS | `auth::router()` correctly chains `.patch(update_me)` |
| No passwords logged or leaked in responses | ✅ PASS | No password fields in any JSON response |
| HTTP 400 for bad input | ❌ FAIL | Missing: no input validation for empty new_username or short new_password (spec requires these checks) |
| HTTP 401 for wrong current password | ⚠️ DEVIATION | Returns 401 (UNAUTHORIZED) — spec specifies 403 (FORBIDDEN) for wrong current password. 401 implies "not authenticated" which is misleading when the session is valid |
| Response body on success | ⚠️ DEVIATION | Returns `{"ok": true}` — spec specifies `{"status": "ok", "reauth_required": true}` |

### Frontend (`user_menu.rs`)

| Item | Status | Notes |
|------|--------|-------|
| `GET /api/v1/auth/me` fetched on mount | ✅ PASS | `LocalResource::new(|| async move { fetch_me().await })` |
| Avatar shows first letter of username | ✅ CODE | Logic is correct but **broken at runtime** due to critical deserialization issue (see §3) |
| Dropdown toggle works via signal | ✅ PASS | `set_dropdown_open.update(|v| *v = !*v)` |
| Logout POSTs to correct endpoint and redirects | ✅ PASS | `POST /api/v1/auth/logout` + `set_href("/login")` |
| Account Settings modal opens/closes | ✅ PASS | `set_modal_open` signal; Cancel resets all state |
| PAM mode: shows "managed by OS" message, hides fields | ⛔ CRITICAL | **Will always show PAM message** due to deserialization mismatch (see §3.1) |
| Local mode: shows username + password change fields | ⛔ CRITICAL | **Never shown** for same reason |
| PATCH called with correct body | ✅ PASS | Sends `current_password` + optional `new_username`/`new_password` |
| Success redirects to `/login` after delay | ✅ PASS | `gloo_timers::Timeout::new(1500, …).forget()` |
| Error messages displayed | ✅ PASS | `save_error` signal drives `.error-msg` paragraph |
| Leptos 0.8 APIs | ✅ PASS | `LocalResource`, `signal()`, `Effect`, `Either`, `spawn_local`, `event_target_value` — all correct Leptos 0.8 CSR patterns |

### Integration

| Item | Status | Notes |
|------|--------|-------|
| `UserMenu` exported from `mod.rs` | ✅ PASS | `pub use user_menu::UserMenu;` |
| `metric_bar.rs` renders `<UserMenu />` right-aligned | ✅ PASS | `<div style="margin-left: auto;"><UserMenu /></div>` at end of metric-bar |

---

## 3. Critical Issues

### 3.1 ⛔ CRITICAL — JSON Deserialization Mismatch (Frontend broken)

**File:** `crates/vexboard-frontend/src/components/user_menu.rs`

**Problem:** The backend's `GET /api/v1/auth/me` returns:
```json
{ "user": { "username": "admin", "auth_mode": "local" } }
```

The frontend attempts to deserialize this directly as `MeResponse`:
```rust
#[derive(Debug, Clone, Deserialize, Default)]
struct MeResponse {
    username: String,
    auth_mode: String,
}

// ...
r.json::<MeResponse>().await.unwrap_or_default()
```

`serde` cannot find `username` or `auth_mode` at the top level of the JSON object (they are nested under `"user"`). By default serde ignores unknown fields and applies defaults for missing ones, so `MeResponse::default()` is silently returned — meaning:
- `username` → `""` (empty string)
- `auth_mode` → `""` (empty string)

**Runtime consequences:**
1. Avatar letter is always blank
2. Username never appears in the trigger button or dropdown header
3. `m.auth_mode == "local"` is always `false` — so the modal **always** shows the PAM "managed by OS" notice, even in local/Docker mode. The username/password form is never rendered for local users.
4. `on_save` sends `current_password` but not `new_username`/`new_password` (since `auth_mode != "local"`) — saving credentials is effectively disabled for all deployments.

**Fix (option A — preferred, matches spec exactly):** Add a wrapper struct:
```rust
#[derive(Debug, Clone, Deserialize, Default)]
struct MeWrapper {
    user: MeResponse,
}

async fn fetch_me() -> MeResponse {
    match gloo_net::http::Request::get("/api/v1/auth/me").send().await {
        Ok(r) if r.ok() => r
            .json::<MeWrapper>()
            .await
            .map(|w| w.user)
            .unwrap_or_default(),
        _ => MeResponse::default(),
    }
}
```

**Fix (option B):** Change the backend to return flat JSON without the `"user"` wrapper. However this breaks consistency with the established pattern used by `login` and other endpoints in `auth.rs`.

### 3.2 ⛔ CRITICAL — `cargo fmt` Fails (exit 1)

**Files:** `user_menu.rs`, `auth.rs`

5 rustfmt diffs prevent the format check from passing. The preflight script enforces `cargo fmt --all -- --check`. Fix: run `cargo fmt --all`.

---

## 4. Recommended Improvements

### 4.1 RECOMMENDED — Wrong HTTP Status for Incorrect Current Password

**File:** `crates/vexboard-server/src/api/auth.rs`

Implementation returns `StatusCode::UNAUTHORIZED` (401); spec requires `StatusCode::FORBIDDEN` (403). 401 conventionally means "not authenticated" which is misleading — the session is valid, the current password is simply wrong. The frontend should also distinguish 401 (session expired, redirect to login) from 403 (wrong current password, show inline error). Current frontend shows both as the raw error body string, which will partially work either way, but the semantic mismatch degrades UX.

### 4.2 RECOMMENDED — Missing Input Validation

**File:** `crates/vexboard-server/src/api/auth.rs` (local `update_me`)

Spec requires:
- Reject empty new username: `return 400 {"error": "Username cannot be empty"}`
- Reject new password shorter than 8 characters: `return 400 {"error": "Password must be at least 8 characters"}`

Implementation applies no such checks; malformed input passes through to the database layer.

### 4.3 RECOMMENDED — Response Body Mismatch on Success

Spec: `{"status": "ok", "reauth_required": true}`  
Implementation: `{"ok": true}`

Minor — the frontend does not use the response body on success (it checks `r.ok()` only), but the spec contract is not met.

### 4.4 RECOMMENDED — PAM `on_save` Sends Unnecessary Network Request

When `auth_mode == "pam"` the Save button is rendered and callable. Clicking it with an empty `auth_mode` (due to bug 3.1) will still issue a `PATCH /api/v1/auth/me` request that the server correctly rejects with 405. After bug 3.1 is fixed, in PAM mode the Save button should either be hidden or disabled. Currently the modal always renders the "Current Password" field and Save button regardless of mode.

---

## 5. Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 62% | D |
| Best Practices | 72% | C |
| Functionality | 45% | F |
| Code Quality | 78% | C+ |
| Security | 88% | B+ |
| Performance | 92% | A |
| Consistency | 82% | B |
| Build Success | 60% | D |

**Overall Grade: D (68%)**

> Build success is penalized because `cargo fmt` fails (the preflight script enforces this as a hard check) and the frontend feature is non-functional at runtime due to the deserialization mismatch. Backend release build and clippy both pass cleanly.

---

## 6. Summary

Two issues block delivery:

1. **JSON deserialization mismatch** — `fetch_me()` in `user_menu.rs` deserializes `{ "user": { ... } }` as a flat `MeResponse`, silently getting empty strings. This means `auth_mode` is always `""`, the username is never shown, and the Account Settings modal always renders the PAM "managed by OS" view regardless of deployment mode. The credential-change form is permanently invisible to local users. Fix requires adding a `MeWrapper { user: MeResponse }` struct.

2. **`cargo fmt` failure** — 5 diffs across `user_menu.rs` and `auth.rs`. Fix: `cargo fmt --all`.

The backend logic (bcrypt verify, uniqueness check, session flush, 405 PAM guard, route wiring) is otherwise sound. Clippy and the backend release build pass cleanly.
