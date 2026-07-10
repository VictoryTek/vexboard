# Auth Mode Settings Toggle — Review

## Scope

Reviewed against `auth_mode_toggle_spec.md`. Modified files:
- `crates/vexboard-server/src/db/mod.rs` — `get_setting`/`set_setting` helpers
- `crates/vexboard-server/src/main.rs` — DB override applied before router build, secret-length re-validation
- `crates/vexboard-server/src/api/settings.rs` — new module, `GET`/`PATCH /api/v1/settings/auth-mode`
- `crates/vexboard-server/src/api/mod.rs` — route registration under `admin_protected`
- `crates/vexboard-server/src/api/openapi.rs` — OpenAPI paths/schemas/tag
- `crates/vexboard-frontend/src/pages/settings.rs` — "Login" section with plain-language Require/Disable buttons and restart banner

## Findings

1. **Specification Compliance** — Matches spec: DB-backed override (not TOML), admin-gated, restart-required surfaced, no live router refactor, no extra confirmation step for none→session per user's explicit decision. Labels changed to "Require Login" / "Disable Login" per user's follow-up request instead of raw "session"/"none" mode names — internal API values are unchanged (`"session"`/`"none"`), only UI copy differs, which is the correct scope (API/DB schema stability vs. UI wording are independent concerns).
2. **Best Practices** — Follows existing admin-route pattern (`require_admin` layer, same as `users::router()`), existing audit-log pattern (`db::audit::insert` on the mutating action, matching `users.rs`), existing upsert style consistent with SQLite `ON CONFLICT`.
3. **Consistency** — New frontend section reuses `settings-nav-option`/`settings-nav-option-active`/`settings-nav-dot` classes already defined for the sidebar-mode picker — no new CSS introduced, matches "Surgical Changes" principle (CLAUDE.md).
4. **Completeness** — Startup override, secret re-validation, GET status, PATCH mutation, audit logging, OpenAPI docs, frontend UI, restart banner — all present per spec.
5. **Security** — Endpoint is admin-only (`require_admin` middleware, same gate as user management); mutation is audit-logged; secret-length check is re-run after the DB override is applied so a `"none"`→`"session"` transition can't silently boot with a sub-32-byte secret — this closes the one security gap identified during Phase 1 research.
6. **Performance** — One extra `SELECT` at startup only (negligible); no per-request overhead since the router is still built once, matching the "restart required" design explicitly chosen by the user.
7. **API Currency** — No new external dependencies; internal `axum`/`sqlx`/`utoipa`/`leptos`/`gloo-net` usage matches existing patterns already present in the crate (no Context7 lookup required per CLAUDE.md's internal-change exemption).
8. **Build Validation:**

   | Command | Result |
   |---|---|
   | `cargo fmt --all -- --check` | Pass — no output, no diff |
   | `cargo clippy --workspace -- -D warnings` | Pass — `Finished` cleanly, 0 warnings |
   | `cargo test -p vexboard-server` | Pass — 34/34 tests passed |
   | `cargo build --release --bin vexboard-server` | Pass — `Finished` release profile |
   | `trunk build --release` (frontend) | **Not run** — `wasm32-unknown-unknown` target and `trunk` CLI are both absent from this machine; running either is explicitly forbidden by CLAUDE.md without confirmed toolchain presence. Frontend change reviewed manually instead (see below). |

### Frontend manual review (in lieu of a WASM build)

- `AuthModeStatus` struct added to `settings.rs` deserializes only `stored_mode`/`restart_required`; backend response includes an extra `active_mode` field which `serde` silently ignores by default (no `deny_unknown_fields`) — safe.
- `set_login_required` closure captures only `RwSignal<String>`/`RwSignal<bool>` (both `Copy` in Leptos 0.8) by move, so it implicitly implements `Copy` and can be used in two separate `on:click` handlers exactly like the existing `mode_for_click`/`mode_for_class` pattern a few lines above it in the same file.
- No new imports required beyond what's already imported at the top of the file (`spawn_local`, `serde_json`, `gloo_net` are all already in scope from the pre-existing "Add User" handler).

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 95% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 90% | A- |

**Overall Grade: A (98%)**

Functionality/Build Success are marked slightly below 100% only because the frontend half of the change could not be exercised by an actual `trunk build` in this environment (toolchain not installed) — code was verified by manual pattern-matching against already-compiling code in the same file, not by compiler.

## PASS / NEEDS_REFINEMENT

**PASS**, with the caveat above. Recommend the user run `trunk build --release` themselves (or via CI, once one exists) before merging, to get compiler-verified confirmation of the frontend half.
