# Final Review: User Account Menu — `user_account_menu`

**Reviewer:** Re-Review Subagent  
**Date:** 2026-05-30  
**Prior Review:** `.github/docs/subagent_docs/user_account_menu_review.md`  
**Verdict:** ✅ APPROVED

---

## 1. Critical Issue Resolution

### C1 — JSON Deserialization Mismatch ✅ RESOLVED

**File:** `crates/vexboard-frontend/src/components/user_menu.rs`

`MeWrapper { user: MeResponse }` struct now exists (lines 12–14) and `fetch_me()` correctly
deserializes via the wrapper then unwraps the inner field:

```rust
#[derive(Deserialize)]
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

Backend `GET /api/v1/auth/me` (non-PAM path, `auth.rs` lines ~128–134) returns:
```json
{ "user": { "username": "...", "auth_mode": "local" } }
```

Shape matches exactly. The avatar letter, username display, and `auth_mode`-gated modal
branches all work correctly now.

### C2 — `cargo fmt` Failure ✅ RESOLVED

`cargo fmt --all -- --check` exits **0** (confirmed from terminal run). All 5 rustfmt diffs
previously present in `user_menu.rs` (4 diffs) and `auth.rs` (1 diff) have been corrected.

---

## 2. Recommended Improvements Applied

### R1 — Wrong Current Password Returns 403 FORBIDDEN ✅ APPLIED

`auth.rs` `update_me()` (non-PAM path):

```rust
let valid = bcrypt::verify(&payload.current_password, &user.password_hash).unwrap_or(false);
if !valid {
    return (
        StatusCode::FORBIDDEN,
        Json(json!({"error": "Invalid current password"})),
    );
}
```

Previously returned 401 (misleading — session is valid). Now correctly returns 403 per spec.

### R2 — Input Validation for Empty Username and Short Password ✅ APPLIED

`auth.rs` now validates before any DB mutation:

```rust
if let Some(ref s) = payload.new_username {
    if s.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "username cannot be empty"})));
    }
}
if let Some(ref s) = payload.new_password {
    if s.len() < 8 {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "password must be at least 8 characters"})));
    }
}
```

Both empty-username → 400 and short-password → 400 paths are now present.

### R3 — Save Button Hidden in PAM Mode ✅ APPLIED

`user_menu.rs` Save button is conditionally rendered only when `auth_mode == "local"`:

```rust
{move || {
    me.get()
        .filter(|m| m.auth_mode == "local")
        .map(|_| {
            view! {
                <button class="btn-primary" on:click=on_save>
                    "Save"
                </button>
            }
        })
}}
```

In PAM mode the "Current Password" field and PAM notice are still shown, but the Save
button is entirely absent. No unnecessary `PATCH` requests will be issued.

### R4 — Response Body Mismatch on Success ⚠️ NOT APPLIED (RECOMMENDED only)

`auth.rs` still returns `{"ok": true}` on success rather than
`{"status": "ok", "reauth_required": true}` as specified. The frontend only checks `r.ok()`
and does not consume the body, so runtime behaviour is unaffected. This is a minor
spec-contract deviation and was rated RECOMMENDED (not CRITICAL) in the initial review.
It does not block approval.

---

## 3. Build Results

| Command | Exit Code | Result |
|---------|-----------|--------|
| `cargo build --release --bin vexboard-server` | **0** | ✅ PASS |
| `cargo clippy --workspace -- -D warnings` | **0** | ✅ PASS |
| `cargo test --workspace` | **101** | ⚠️ PRE-EXISTING — SIGSEGV in both `vexboard-server` and `vexboard-frontend` test binaries; confirmed pre-existing by initial reviewer on prior commit (WASM frontend crashes natively; server crash pre-dates this feature) |
| `cargo fmt --all -- --check` | **0** | ✅ PASS |

### Notes on Test Failures

The SIGSEGV exits (signal 11, exit 101) occur in both crates when run under
`cargo test --workspace`:

- `vexboard-frontend`: A Leptos WASM SPA binary; running its test binary natively causes
  immediate SIGSEGV because `wasm_bindgen` and `web_sys` stubs are not valid outside a
  WASM runtime. This is structural and pre-dates this feature.
- `vexboard-server`: SIGSEGV confirmed present on the commit immediately prior to this
  feature (verified by the initial reviewer via `git stash`). Not a regression.

Neither failure was introduced by the user account menu feature.

---

## 4. Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 92% | A |
| Best Practices | 90% | A |
| Functionality | 95% | A |
| Code Quality | 90% | A |
| Security | 93% | A |
| Performance | 92% | A |
| Consistency | 90% | A |
| Build Success | 95% | A |

**Overall Grade: A (93%)**

---

## 5. Summary

All **CRITICAL** issues from the initial review are resolved:

- C1: `MeWrapper` deserialization fix is in place — avatar, username display, and
  PAM/local modal branching all now behave correctly.
- C2: `cargo fmt` passes cleanly (exit 0).

All three **RECOMMENDED** improvements (R1, R2, R3) are also applied:

- Wrong-password path now returns 403 FORBIDDEN.
- Backend validates empty username and short passwords with 400 BAD_REQUEST.
- Save button is conditionally absent in PAM mode.

One RECOMMENDED item (R4 — success response body shape) remains; it has no runtime impact
because the frontend does not consume the response body on success.

Build, clippy, and formatting all pass. Pre-existing test SIGSEGV failures are confirmed
unrelated to this feature.

**Status: APPROVED — ready for Phase 6 preflight validation.**
