# FEAT-2 — Dismiss discovered services — Review

## Spec Reference

`.github/docs/subagent_docs/FEAT-2_dismiss_discovered_services_spec.md`

## Changes Reviewed

- `crates/vexboard-server/src/db/migrations/005_dismissed_units.sql` (new)
- `crates/vexboard-server/src/db/mod.rs`
- `crates/vexboard-server/src/discovery/mod.rs`
- `crates/vexboard-server/src/discovery/systemd.rs`
- `crates/vexboard-server/src/discovery/docker.rs`
- `crates/vexboard-server/src/api/openapi.rs`
- `crates/vexboard-frontend/src/components/discovery_panel.rs`

## 1. Specification Compliance

All implementation steps in the spec were followed:
- `dismissed_units` table added via migration 005, wired unconditionally (idempotent `CREATE TABLE IF NOT EXISTS`), matching the `002_audit_log.sql` style rather than the probe-gated `ALTER TABLE` style — correct, since this is a new table, not a new column.
- `POST`/`DELETE /api/v1/discovery/dismiss` added to `discovery::router()`; already covered by the existing `admin_protected` nest in `api/mod.rs` — no router-wiring change needed there, as predicted.
- `systemd::discover_units` filters on `dismissed_units WHERE source = 'systemd'` at the same point as the existing claimed-check.
- `docker::discover_containers`/`discover_from_socket` filters on `(source, unit_name)` pairs for `docker`/`podman` — improved over the spec's name-only suggestion to avoid a same-name cross-source false positive (docker container "app" dismissed shouldn't suppress a podman container also named "app").
- Frontend "Dismiss" button added beside "Add", posts to the new endpoint, then refetches — matches spec.
- `DismissRequest` and both new handlers registered in `openapi.rs` paths/schemas (not explicitly called out in the spec's Implementation Steps but required for OpenAPI/Swagger correctness and consistent with how every other endpoint in this codebase is documented).

## 2. Best Practices

- Follows the established handler pattern (`quick_links.rs`): `sqlx::query` + manual `(StatusCode, Json)` match, `db::audit::insert` per mutation, `#[utoipa::path]` docs, `#[tracing::instrument]`.
- `INSERT OR IGNORE` correctly handles the `UNIQUE(source, unit_name)` constraint without erroring on duplicate dismiss calls.
- Error paths return 500 with a logged `tracing::error!`, consistent with sibling modules.

## 3. Consistency

- Route registration (`post(...).delete(...)` on a single path) matches the existing `quick_links.rs` `put(...).delete(...)` style.
- Audit action names (`discovery.dismiss`, `discovery.undismiss`) follow the `resource.verb` convention used throughout (`quick_link.create`, `discovery.refresh`).
- Frontend button styling (ghost/secondary look for Dismiss vs `btn-primary` for Add) matches the spec's guidance to keep Add primary.

## 4. Maintainability

- Small, self-contained diff; no unrelated refactors.
- `DismissRequest` DTO reused for both dismiss and undismiss bodies — appropriate, avoids duplicate near-identical structs for a 2-field payload.

## 5. Completeness

- Backend dismiss + filtering + un-dismiss all implemented.
- Frontend only surfaces "Dismiss" (not "un-dismiss") — intentional per spec: no dismissed-units management UI was in scope; the DELETE endpoint is documented API surface for future use, consistent with how other admin endpoints in this codebase (e.g. audit log) shipped before their UI.

## 6. Performance

- One extra `SELECT` per discovery pass per source family (systemd: 1 query; docker: 1 query covering both docker+podman) — negligible, in line with the existing per-pass "claimed" query pattern.
- Dismiss handler holds the `discoveries` write lock only for the `retain` call, released immediately via explicit `drop` before the audit-log await — avoids holding the lock across an `.await`.

## 7. Security

- Both new endpoints inherit `admin_protected` + `require_admin` middleware — no unauthenticated or viewer-level access, matching every other discovery-mutation endpoint.
- No new attack surface: inputs are bound via `sqlx` parameterized queries (no injection risk), body is a simple 2-field struct.

## 8. API Currency

- No new external dependencies; uses the same `axum`/`sqlx`/`utoipa` patterns already present in the codebase at current pinned versions. Context7 lookup not required per the Dependency Policy exemption for internal changes with no new dependencies.

## 9. Build Validation

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Initially failed (4 formatting diffs in `discovery/mod.rs`); fixed via `cargo fmt --all`; re-check passes clean |
| `cargo clippy --workspace -- -D warnings` | Pass — 0 warnings (also natively compiles `vexboard-frontend`'s Leptos CSR code with no errors, giving partial confidence in the frontend changes despite no `trunk build`) |
| `cargo test -p vexboard-server` | Pass — 34/34 tests, no SIGSEGV |
| `cargo build --release --bin vexboard-server` | Pass — clean release build |
| `cargo audit --ignore RUSTSEC-2023-0071` | Skipped — `cargo-audit` not installed on this machine |
| `trunk build` | Not run — Trunk CLI and `wasm32-unknown-unknown` target both confirmed absent; per FORBIDDEN COMMANDS this requires explicit approval before installing/running |

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
| Build Success | 95% | A |

**Overall Grade: A (98.75%)**

(Functionality/Build Success scored 95% rather than 100% only because the frontend WASM artifact could not be verified with an actual `trunk build`/browser test — Trunk is not installed in this environment. `cargo clippy --workspace` did successfully native-compile the Leptos component with zero errors, which substantially de-risks this gap.)

## Result

**PASS**
