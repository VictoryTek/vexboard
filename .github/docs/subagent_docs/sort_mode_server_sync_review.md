# Sort Mode Server Sync — Review

## Scope Reviewed

- `crates/vexboard-server/src/api/auth.rs` — new `update_sort_mode` handler, `UpdateSortModeRequest` DTO, route registration, `me` handler extended.
- `crates/vexboard-server/src/api/openapi.rs` — new path + schema registered.
- `crates/vexboard-server/src/tests.rs` — `put_json` test helper, 4 new tests covering default value, persistence round-trip, invalid-value rejection, unauthenticated rejection.
- `crates/vexboard-frontend/src/pages/dashboard/mod.rs` — `localStorage` helpers replaced with server fetch/save, signal load wired via `LocalResource` + `Effect`, click handler now `spawn_local`s the save.

## Findings

1. **Specification Compliance** — Implementation matches `sort_mode_server_sync_spec.md` exactly: reused the existing `settings` KV table with a `dashboard_sort_mode:{username}` key (no migration added), added `PUT /api/v1/auth/me/sort-mode`, extended `GET /api/v1/auth/me`, and replaced the frontend `localStorage` helpers with the described fetch/save functions and `LocalResource`/`Effect` wiring.
2. **Best Practices** — Handler follows the exact idiom already used by `update_me`/`logout` (session-derived username, `(StatusCode, Json(json!(...)))` tuples, `tracing::instrument`, `#[utoipa::path]` doc block). Server-side validation of the 3 allowed `sort_mode` values prevents storing garbage that would silently fall back to `AZ` on read.
3. **Consistency** — Frontend fetch/save functions mirror the codebase's established per-component `gloo_net::http::Request` pattern (matches `reorder_services`, `modal_groups.rs`, `settings.rs`) rather than introducing a shared client abstraction. `cargo fmt` reformatted the one multi-line `match` into the project's standard single-line chain style.
4. **Completeness** — Both read (`me`) and write (`/me/sort-mode`) sides are covered; UI click handler now persists via the network instead of a synchronous local write.
5. **Security** — New endpoint requires an authenticated session (401 if not); no admin gate needed since the value is scoped to the caller's own username, matching the intended "each user controls their own preference" design. No SQL injection risk (bound parameters via `sqlx::query`/`query_scalar` throughout `get_setting`/`set_setting`).
6. **Performance** — One extra tiny SQLite point-lookup per `/me` call; negligible.
7. **Known limitation (documented in spec, not a defect):** a brief `AZ` flash on dashboard load until the async `fetch_sort_mode` resolves (previously instant via synchronous `localStorage` read). Accepted trade-off, consistent with how `services`/`quick_links`/`groups` already load via `LocalResource` on this page.
8. **PAM/local auth parity** — verified the key is namespaced by `username` (not `user_id`/FK to `users`), so it works identically for PAM-authenticated users, who have no row in the local `users` table.

## Build Validation

All commands run from the repo root; `cargo clippy --workspace`/`cargo build --workspace` were **not** run (would attempt native compilation of the WASM-only `vexboard-frontend` crate — forbidden per project constraints); scoped to `-p vexboard-server` / `--bin vexboard-server` instead, consistent with the project's own stated Resource Constraints.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass (one file auto-formatted, then re-verified clean) |
| `cargo clippy -p vexboard-server -- -D warnings` | Pass, 0 warnings |
| `cargo test -p vexboard-server` | Pass — 40/40 tests (4 new: `test_sort_mode_defaults_to_az_when_unset`, `test_update_sort_mode_persists_and_reflects_in_me`, `test_update_sort_mode_rejects_invalid_value`, `test_update_sort_mode_unauthenticated_returns_401`) |
| `cargo build --release --bin vexboard-server` | Pass |

**Frontend build not run**: `trunk` CLI and the `wasm32-unknown-unknown` target are not installed on this machine (confirmed via `rustup target list --installed` / `which trunk`), and `trunk build`/`trunk serve` are listed as forbidden without that confirmation. The frontend diff was reviewed manually for correctness (import availability, `Effect`/`spawn_local` already used identically elsewhere in the same file, `gloo_net` call shape matches existing call sites across the codebase byte-for-byte in structure).

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 95% | A (frontend unverified by compiler — WASM toolchain unavailable; manually reviewed) |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 95% | A (backend fully verified; frontend build environment unavailable) |

**Overall Grade: A (98.75%)**

## Result

**PASS**
