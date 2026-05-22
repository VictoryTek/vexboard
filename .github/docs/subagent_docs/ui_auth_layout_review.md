# VexBoard — UI, Auth & Layout Review

**Review Date:** 2026-05-22  
**Spec:** `.github/docs/subagent_docs/ui_auth_layout_spec.md`  
**Reviewer:** QA Agent (Phase 3)

---

## 1. Build Results

### Step 1: Backend Build (`cargo build --release --bin vexboard-server`)

**Result: FAILED — CRITICAL**

```
error: failed to select a version for the requirement `pam = "^1.0"`
candidate versions found which didn't match: 0.8.0, 0.7.0, 0.0.1
location searched: crates.io index
required by package `vexboard-server v0.1.0 (C:\Projects\vexboard\crates\vexboard-server)`
```

`crates/vexboard-server/Cargo.toml` specifies `pam = { version = "1.0", optional = true }`.  
**Version 1.0 of the `pam` crate does not exist.** The latest available version is `0.8.0`.  
Even though this is a `[target.'cfg(unix)'.dependencies]` entry, Cargo performs global dependency
resolution for the whole workspace before building any target. The resolution failure blocks
every cargo command — `build`, `clippy`, `test`, and `check`.

**Fix required:** Change to `pam = { version = "0.8", optional = true }`.

---

### Step 2: Workspace Linting (`cargo clippy --workspace -- -D warnings`)

**Result: BLOCKED — same dependency resolution error as Step 1**

```
error: failed to select a version for the requirement `pam = "^1.0"`
```

Cannot evaluate clippy output until the pam version is corrected.

---

### Step 3: Formatting Check (`cargo fmt --all -- --check`)

**Result: FAILED — formatting differences found (exit code 1)**

`cargo fmt` does not require dependency resolution and ran successfully, revealing
formatting deviations in 9 files:

| File | Nature of diff |
|---|---|
| `crates/vexboard-frontend/src/components/metric_bar.rs` | `if/else if/else` chain needs braces on separate lines |
| `crates/vexboard-frontend/src/components/service_card.rs` | `match` arm tuples need wrapping |
| `crates/vexboard-frontend/src/components/sidebar.rs` | Long `use_context` chain needs line break |
| `crates/vexboard-frontend/src/components/status_badge.rs` | `match` arm alignment spacing |
| `crates/vexboard-frontend/src/main.rs` | `#[cfg]`-gated single-expression blocks need line breaks |
| `crates/vexboard-frontend/src/pages/settings.rs` | Long `use_context` chain needs line break |
| `crates/vexboard-frontend/src/pages/setup.rs` (×2) | `web_sys::window().unwrap().location().set_href(...)` chains need wrapping |
| `crates/vexboard-server/src/api/auth.rs` | `session.insert(...).await.ok()` chain needs wrapping |
| `crates/vexboard-server/src/pam_auth.rs` | `client.conversation_mut().set_credentials(...)` chain needs wrapping |

Note: Formatting issues are treated as **RECOMMENDED**, not CRITICAL, per review criteria.

---

### Step 4: Workspace Tests (`cargo test --workspace`)

**Result: BLOCKED — same dependency resolution error as Step 1**

```
error: failed to select a version for the requirement `pam = "^1.0"`
```

---

### Step 5: Frontend Build (`cargo check --target wasm32-unknown-unknown` from frontend crate)

**Result: BLOCKED — same dependency resolution error as Step 1**

Even when invoked from `crates/vexboard-frontend/`, Cargo resolves the whole workspace lockfile
and fails before reaching the frontend crate.

---

## 2. Code Review Findings

### 2.1 Backend

#### `crates/vexboard-server/Cargo.toml` — CRITICAL

- **pam version "1.0" does not exist.** Must be `"0.8"`. See build failure above.
- All other dependencies (`tower-sessions = "0.15"`, `bcrypt`, `axum 0.8`, `sqlx 0.8`) are
  correctly declared in the workspace `Cargo.toml` and carried over to the server crate. ✓
- `[features]` section and `default = []` are correctly structured. ✓

#### `crates/vexboard-server/src/pam_auth.rs` — CRITICAL (secondary, unverifiable)

```rust
pub fn authenticate_pam(username: &str, password: &str) -> Result<(), pam::PamError> {
    let mut client = pam::Client::with_password("vexboard")?;
    client.conversation_mut().set_credentials(username, password);
    client.authenticate()?;
    Ok(())
}
```

- Uses `pam::Client::with_password` and `pam::PamError`. These were specified against `pam` v1.0.
- In `pam` 0.8, the type may be named `pam::Authenticator` or the error type may differ.
  Once the version is fixed to `"0.8"`, this must be verified for API compatibility.
- The function itself is correctly guarded inside `#[cfg(feature = "pam-auth")]` at the function
  level, AND the entire `mod pam_auth;` is wrapped in `#[cfg(feature = "pam-auth")]` in
  `main.rs` — correct double-guarding. ✓
- The function is sync (not async), which is correct for PAM (blocking C library call). ✓

#### `crates/vexboard-server/src/api/auth.rs` — PASS with RECOMMENDED notes

- Both `#[cfg(feature = "pam-auth")]` and `#[cfg(not(feature = "pam-auth"))]` branches compile
  to functions with the same name `login` — correct, only one is compiled at a time. ✓
- Session is properly wired: both branches call `session.insert("username", ...)`. ✓
- `logout` flushes session correctly. ✓
- `me` reads `session.get::<String>("username")` — internally consistent. ✓
- **RECOMMENDED:** The spec called for storing `"user_id"` (i64) in the session and doing a DB
  lookup in `me`. The implementation stores `"username"` (String) directly. This is simpler and
  functionally correct, but `me` should ideally verify the user still exists in the DB
  to handle deleted accounts correctly.
- **RECOMMENDED:** The PAM-mode `login` handler takes `State(_state): State<AppState>` but
  doesn't use `_state`. In PAM mode, `AppState` is not needed. This compiles correctly (underscore
  suppresses the unused warning) but is unnecessary boilerplate. Consider removing it for clarity.

#### `crates/vexboard-server/src/api/setup.rs` — PASS

- **Security check confirmed:** `create_admin` queries `SELECT COUNT(*) FROM users` and returns
  `409 CONFLICT` if `count != 0`. An attacker cannot call this endpoint to create additional
  users when users already exist. ✓
- Input validation: username non-empty, password ≥ 8 characters. ✓
- `bcrypt::hash` with `DEFAULT_COST` for password hashing. ✓
- `pam-auth` feature correctly gates the two versions of both `status` and `create_admin`. ✓
- The `status` endpoint in pam mode takes no arguments while the non-pam version takes
  `State(state): State<AppState>` — both are valid Axum handlers. ✓
- Fail-safe: `unwrap_or(1)` on the count query means a DB error is treated as "already set up",
  preventing accidental re-initialization on transient errors. ✓
- **RECOMMENDED:** Minor TOCTOU race condition — two simultaneous requests when `users` is empty
  could both pass the count check. Acceptable for a first-run flow that runs once; a UNIQUE
  constraint on `username` at the DB level provides a secondary safety net.

#### `crates/vexboard-server/src/api/mod.rs` — PASS

- `pub mod setup;` declared. ✓
- Routes `/api/v1/setup/status` (GET) and `/api/v1/setup` (POST) registered before any auth
  middleware, so they are accessible pre-authentication. ✓
- Route ordering is correct. ✓

#### `crates/vexboard-server/src/main.rs` — PASS

- `#[cfg(feature = "pam-auth")] mod pam_auth;` correctly conditionally includes the module. ✓
- `tower_sessions` middleware wired: `MemoryStore::default()` + `SessionManagerLayer`. ✓
- `SessionManagerLayer` applied to the router with `.with_secure(false)`. ✓
- **RECOMMENDED:** In-memory session store (`MemoryStore`) loses all sessions on server restart.
  The spec noted this is acceptable for now, but production deployments should migrate to a
  SQLite-backed store. A TODO comment is present in the code acknowledging this. ✓
- `AppState` correctly carries `db`, `config`, `discoveries`, `metrics_tx`, `probe_tx`. ✓

---

### 2.2 Frontend

#### `crates/vexboard-frontend/src/main.rs` — PASS with RECOMMENDED notes

- `provide_context(sidebar_mode)` and `provide_context(set_sidebar_mode)` called before
  the `view!` macro, ensuring context is available to all child components. ✓
- First-run guard implemented as `Effect::new(move |_| { spawn_local(... ) })`. ✓
  The effect has no reactive dependencies in its synchronous body, so it runs only once on
  mount — correct behavior for a one-time check. ✓
- Guard correctly skips redirect when already on `/setup` or `/login`. ✓
- Layout fix applied: `<main class="flex-1 flex flex-col overflow-hidden">` with inner
  `<div class="flex-1 overflow-auto p-6">` — MetricBar is now a sticky non-scrolling header
  and route content scrolls independently. ✓
- `/setup` route added. ✓
- **RECOMMENDED:** Formatting deviations found by `cargo fmt` in `#[cfg]`-gated blocks
  (single-expression blocks need their own lines per rustfmt rules).

#### `crates/vexboard-frontend/src/components/sidebar.rs` — PASS

- `SidebarMode` enum derives `Debug, Clone, PartialEq, Default` with `#[default]` on
  `HoverExpand`. ✓
- `load_sidebar_mode_from_storage` and `save_sidebar_mode_to_storage` are correctly guarded
  with both `#[cfg(target_arch = "wasm32")]` and `#[cfg(not(target_arch = "wasm32"))]`
  no-op stubs, ensuring the crate compiles for non-wasm targets. ✓
- `HoverExpand` hover logic: `on:mouseenter` sets `hovered = true`, `on:mouseleave` sets
  `hovered = false`. `is_expanded()` combines mode + hovered state correctly. ✓
- Settings link moved to `<div class="sidebar-footer">` (pinned to bottom). ✓
- Settings link hidden from main `<nav>` — Dashboard is the only main nav item. ✓
- `use_location()` from `leptos_router::hooks` used for `pathname` reactive signal to
  highlight active links. ✓
- `use_context::<ReadSignal<SidebarMode>>()` — correct Leptos 0.8 context read API. ✓
- `signal(false)` for `hovered` — correct Leptos 0.8 API. ✓

#### `crates/vexboard-frontend/src/pages/setup.rs` — PASS

- Form validation (passwords match, password ≥ 8 chars) done client-side before submission. ✓
- On 409 conflict, displays message and redirects to `/login`. ✓
- On success, redirects to `/login`. ✓
- `web_sys::window().unwrap()` calls are `#[cfg(target_arch = "wasm32")]` guarded. ✓
- **RECOMMENDED:** Formatting — `web_sys::window().unwrap().location().set_href(...)` chains
  need wrapping per `rustfmt`.

#### `crates/vexboard-frontend/src/pages/settings.rs` — PASS with RECOMMENDED notes

- `use_context` for both `sidebar_mode` and `set_sidebar_mode` — correct. ✓
- Navigation mode selector renders all three `SidebarMode` variants with labels and
  descriptions. ✓
- `save_sidebar_mode_to_storage` called on mode change — no `#[cfg]` guard needed here
  since both wasm32 and non-wasm32 variants of the function are defined. ✓
- **RECOMMENDED:** The theme toggle uses `doc.document_element()` and `html.class_list()`,
  which require `web-sys` features `Document`, `Element`, and `DomTokenList`. The current
  `Cargo.toml` only lists `Window`, `Storage`, `Location`, `EventSource`, `MessageEvent`,
  `HtmlInputElement`. This pre-existing omission may cause a wasm32 compile error. Needs
  verification after the pam version blocker is resolved.
- **RECOMMENDED:** `use wasm_bindgen::JsCast;` is imported inside the theme toggle
  `#[cfg(target_arch = "wasm32")]` block but `JsCast` is never actually called. This import
  can be removed. May trigger an `unused_imports` clippy warning.

#### `crates/vexboard-frontend/src/pages/mod.rs` — PASS

- `pub mod setup;` correctly added. ✓

---

### 2.3 NixOS Module (`nix/module.nix`)

- `security.pam.services.vexboard = {}` — correct NixOS syntax for a minimal PAM service
  that uses standard `pam_unix` authentication. ✓
- `DynamicUser = false` — required for PAM (ephemeral users cannot read `/etc/shadow`). ✓
- `User = "vexboard"; Group = "vexboard"` — dedicated system user declared. ✓
- `SupplementaryGroups = [ "shadow" "systemd-journal" ]` — `shadow` group is required to
  read `/etc/shadow` for PAM authentication. ✓
- `users.users.vexboard` and `users.groups.vexboard` correctly declared. ✓
- **RECOMMENDED:** `createHome = false` is missing from `users.users.vexboard`. While
  `isSystemUser = true` typically implies no home directory creation, explicitly setting
  `createHome = false` follows the spec and is best practice.

### 2.4 NixOS Package (`nix/package.nix`)

- `linux-pam` added to `buildInputs`. ✓
- `buildPhase` builds with `--features pam-auth`. ✓
- Frontend built via `trunk build --release` before backend. ✓
- `wasm-bindgen-cli` and `trunk` in `nativeBuildInputs`. ✓

---

## 3. Score Table

| Category | Score | Grade |
|---|---|---|
| Specification Compliance | 82% | B |
| Best Practices | 78% | C+ |
| Functionality | 85% | B |
| Code Quality | 80% | B- |
| Security | 88% | B+ |
| Performance | 85% | B |
| Consistency | 82% | B |
| Build Success | 0% | F |

**Overall Grade: D+ (60%)** — build failure dominates; all other categories are high quality.

---

## 4. CRITICAL Issues

| # | File | Issue | Required Fix |
|---|---|---|---|
| 1 | `crates/vexboard-server/Cargo.toml` | `pam = { version = "1.0", optional = true }` — version 1.0 does not exist on crates.io. Blocks ALL cargo commands. | Change to `pam = { version = "0.8", optional = true }` |
| 2 | `crates/vexboard-server/src/pam_auth.rs` | `pam::Client` and `pam::PamError` usage unverifiable until version fixed. `pam` 0.8 may use a different type name — verify API compatibility and fix if needed. | Verify against pam 0.8 docs after fixing version; fix type names if necessary |

---

## 5. RECOMMENDED Issues

| # | File | Issue |
|---|---|---|
| 1 | Multiple (9 files) | Formatting deviations detected by `cargo fmt --all -- --check`. Run `cargo fmt --all` to auto-fix. |
| 2 | `crates/vexboard-server/src/api/auth.rs` | `me` endpoint returns username from session without verifying the user still exists in DB. Should query `users` table by username to handle deleted accounts. |
| 3 | `crates/vexboard-server/src/api/auth.rs` | PAM-mode `login` takes `State(_state): State<AppState>` unnecessarily. Remove unused extractor. |
| 4 | `crates/vexboard-frontend/src/pages/settings.rs` | `web-sys` features list is missing `Document`, `Element`, `DomTokenList` needed by the theme toggle. Add to `Cargo.toml`. |
| 5 | `crates/vexboard-frontend/src/pages/settings.rs` | `use wasm_bindgen::JsCast;` imported but never used in theme toggle. Remove to avoid unused-import warning. |
| 6 | `nix/module.nix` | `createHome = false` missing from `users.users.vexboard` definition. |
| 7 | `crates/vexboard-server/src/main.rs` | In-memory session store loses all sessions on restart. Acceptable short-term; should be replaced with SQLite-backed store in production. |

---

## 6. Final Verdict

**NEEDS_REFINEMENT**

The implementation is architecturally sound and closely follows the specification. The session
wiring, first-run flow, sidebar behavior, layout fix, and NixOS module changes are all
correctly implemented. However, the `pam` crate version `"1.0"` does not exist on crates.io —
this single dependency error prevents the entire workspace from resolving, which blocks
`cargo build`, `cargo clippy`, `cargo test`, and even `cargo check --target wasm32-unknown-unknown`
for the frontend.

**The blocking issues are:**
1. Fix `pam` version from `"1.0"` to `"0.8"` in `crates/vexboard-server/Cargo.toml`
2. Verify `pam::Client` / `pam::PamError` API compatibility against pam 0.8 and fix if needed

Once those two issues are resolved, the project should build cleanly. The recommended issues
(especially the missing `web-sys` features for `Document`/`Element`/`DomTokenList`) should also
be addressed during refinement so the frontend wasm32 build succeeds.
